#!/usr/bin/env bash
#
# Boot smoke test: start zCore in QEMU, wait for the shell prompt,
# run "poweroff -f" to verify clean shutdown, then exit.
#
# Usage: tools/scripts/boot-test.sh <arch>
#   arch: aarch64 (others may be added later)
#
# Exit code 0 = shell prompt reached and clean shutdown (boot success)
# Exit code 1 = timeout or error (boot failure)

set -euo pipefail

ARCH="${1:?Usage: $0 <arch>}"
TIMEOUT=60
PROMPT_PATTERN='/ # '

case "$ARCH" in
  aarch64)
    KERNEL="target/aarch64/release/zcore"
    IMAGE="zCore/aarch64.img"
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
  x86_64)
    # x86_64 uses a BIOS bootable disk image that bundles kernel + rootfs.
    # Build the boot image from the kernel ELF + rootfs SFS image.
    KERNEL_ELF="target/x86_64/release/zcore"
    BOOT_IMG="target/x86_64/release/boot.img"
    ROOTFS_IMG="zCore/x86_64.img"
    BOOTIMAGE_TOOL="tools/x86-bootimage/target/release/x86-bootimage"

    if [ ! -f "$BOOTIMAGE_TOOL" ]; then
      echo "Building x86-bootimage tool..."
      cargo build --release --manifest-path tools/x86-bootimage/Cargo.toml
    fi
    BOOTIMAGE_ARGS=("$KERNEL_ELF" "$BOOT_IMG")
    if [ -f "$ROOTFS_IMG" ]; then
      BOOTIMAGE_ARGS+=(--ramdisk "$ROOTFS_IMG")
    fi
    "$BOOTIMAGE_TOOL" "${BOOTIMAGE_ARGS[@]}"

    KERNEL="$BOOT_IMG"
    QEMU_CMD=(
      qemu-system-x86_64
      -m 2G -display none -no-reboot -nographic
      -machine q35 -cpu qemu64,+fsgsbase,+rdrand
      -serial mon:stdio
      -drive "format=raw,file=$BOOT_IMG"
    )
    ;;
  *)
    echo "ERROR: boot-test.sh does not yet support arch '$ARCH'"
    exit 1
    ;;
esac

# Verify required files exist
for f in "$KERNEL"; do
  if [ ! -f "$f" ]; then
    echo "ERROR: $f not found. Run 'make build ARCH=$ARCH' first."
    exit 1
  fi
done

echo "Starting QEMU (timeout=${TIMEOUT}s)..."

# Create a temp file for QEMU output and a FIFO for QEMU stdin
OUTPUT=$(mktemp)
QEMU_IN=$(mktemp -u)
mkfifo "$QEMU_IN"
trap 'rm -f "$OUTPUT" "$QEMU_IN"; kill "$QEMU_PID" 2>/dev/null || true' EXIT

# Start QEMU with stdin from the FIFO
"${QEMU_CMD[@]}" < "$QEMU_IN" > "$OUTPUT" 2>&1 &
QEMU_PID=$!

# Keep the FIFO open for writing (background cat holds it open)
exec 3>"$QEMU_IN"

# Poll for the shell prompt
ELAPSED=0
while [ "$ELAPSED" -lt "$TIMEOUT" ]; do
  if grep -q "$PROMPT_PATTERN" "$OUTPUT" 2>/dev/null; then
    echo "Shell prompt reached in ${ELAPSED}s"

    # Send poweroff command and wait for QEMU to exit cleanly
    echo "Sending 'poweroff -f'..."
    echo "poweroff -f" >&3
    exec 3>&-  # close the FIFO write end

    # Wait up to 10s for QEMU to exit
    for i in $(seq 1 10); do
      if ! kill -0 "$QEMU_PID" 2>/dev/null; then
        wait "$QEMU_PID" || true
        echo "PASS: boot + poweroff completed successfully"
        exit 0
      fi
      sleep 1
    done

    echo "FAIL: QEMU did not exit after poweroff"
    echo "--- QEMU output ---"
    cat "$OUTPUT"
    kill "$QEMU_PID" 2>/dev/null || true
    exit 1
  fi

  # Check if QEMU exited early (crash / panic)
  if ! kill -0 "$QEMU_PID" 2>/dev/null; then
    echo "FAIL: QEMU exited before shell prompt was reached"
    echo "--- QEMU output ---"
    cat "$OUTPUT"
    exec 3>&- 2>/dev/null || true
    exit 1
  fi

  sleep 1
  ELAPSED=$((ELAPSED + 1))
done

# Timeout
echo "FAIL: shell prompt not reached within ${TIMEOUT}s"
echo "--- QEMU output ---"
cat "$OUTPUT"
exec 3>&- 2>/dev/null || true
kill "$QEMU_PID" 2>/dev/null || true
exit 1
