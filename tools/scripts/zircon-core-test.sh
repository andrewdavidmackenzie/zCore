#!/usr/bin/env bash
#
# Run Fuchsia Zircon core tests against zCore on x86_64.
#
# This script:
#   1. Copies prebuilt Fuchsia test binaries into the Zircon rootfs
#   2. Builds the kernel + rootfs SFS image
#   3. Boots QEMU with the rootfs
#   4. Runs core-tests-standalone and reports results
#
# Usage: tools/scripts/zircon-core-test.sh
#
# Requires: qemu-system-x86_64, OVMF firmware

set -euo pipefail

ARCH="x86_64"
PREBUILT_DIR="prebuilt/zircon/x86_64"
ROOTFS_DIR="target/rootfs/zircon/$ARCH"
SESSION_TIMEOUT=300  # 5 minutes for the full test suite

# Verify prebuilts exist
if [ ! -f "$PREBUILT_DIR/core-tests-standalone" ]; then
    echo "ERROR: Prebuilt binaries not found at $PREBUILT_DIR/"
    echo "See $PREBUILT_DIR/README.md for how to obtain them."
    exit 1
fi

# Step 1: Set up rootfs with test binaries
echo "==> Setting up Zircon rootfs with Fuchsia test binaries..."
mkdir -p "$ROOTFS_DIR/bin"
mkdir -p "$ROOTFS_DIR/lib"

# Copy test binary
cp "$PREBUILT_DIR/core-tests-standalone" "$ROOTFS_DIR/bin/"

# Copy shared libraries (dynamic linker + dependencies)
for lib in ld.so.1 libc.so libzircon.so libc++.so.2 libc++abi.so.1 libinspector.so libmini-process.so; do
    cp "$PREBUILT_DIR/$lib" "$ROOTFS_DIR/lib/"
done

echo "   Copied test binary + 7 shared libraries to rootfs"

# Step 2: Build rootfs SFS image (increased size for test binaries)
echo "==> Building rootfs SFS image..."
# The xtask zircon-rootfs command builds petal programs too, but we
# just need the SFS image. Force rebuild by removing the old image.
ROOTFS_IMG="target/qemu-$ARCH/release/$ARCH-zircon.img"
rm -f "$ROOTFS_IMG"
cargo xtask zircon-rootfs --arch "$ARCH" 2>&1 | tail -3

if [ ! -f "$ROOTFS_IMG" ]; then
    echo "ERROR: rootfs image not found at $ROOTFS_IMG"
    exit 1
fi

# Step 3: Build kernel for Zircon mode
echo "==> Building kernel..."
ZCORE_CMDLINE="LOG=warn ROOTPROC=/bin/core-tests-standalone" cargo bin -m qemu-$ARCH 2>&1 | tail -3

KERNEL_ELF="target/qemu-$ARCH/release/kernel"
BOOT_IMG="target/qemu-$ARCH/release/boot.img"

# Build UEFI boot image with rootfs embedded
BOOTIMAGE_TOOL="tools/x86-bootimage/target/release/x86-bootimage"
if [ ! -f "$BOOTIMAGE_TOOL" ]; then
    cargo build --release --manifest-path tools/x86-bootimage/Cargo.toml
fi
"$BOOTIMAGE_TOOL" "$KERNEL_ELF" "$BOOT_IMG" --ramdisk "$ROOTFS_IMG"

# Find OVMF firmware
source "$(dirname "$0")/find-ovmf.sh"
OVMF=$(find_ovmf) || { echo "ERROR: OVMF not found"; exit 1; }

# Step 4: Run tests in QEMU
echo "==> Running Zircon core tests in QEMU (timeout=${SESSION_TIMEOUT}s)..."
TMPDIR_QEMU=$(mktemp -d)
OUTPUT="$TMPDIR_QEMU/output"
touch "$OUTPUT"

qemu-system-x86_64 \
    -m 4G -display none -no-reboot -nographic \
    -machine q35 -cpu qemu64,+fsgsbase,+rdrand,+sse3,+ssse3,+sse4.1,+sse4.2,+popcnt,+cx16 \
    -serial mon:stdio \
    -drive if=pflash,format=raw,readonly=on,file="$OVMF" \
    -drive "format=raw,file=$BOOT_IMG" \
    > "$OUTPUT" 2>&1 &
PID=$!

# Wait for completion or timeout
W=0
completed=false
while [ "$W" -lt "$SESSION_TIMEOUT" ]; do
    if ! kill -0 "$PID" 2>/dev/null; then
        completed=true
        break
    fi
    # Check for the final suite summary line that marks all tests done
    if grep -q '^\[==========\]' "$OUTPUT" 2>/dev/null; then
        # Give it a moment to finish output
        sleep 5
        completed=true
        kill "$PID" 2>/dev/null || true
        break
    fi
    sleep 2
    W=$((W + 2))
done

timed_out=false
if ! $completed && kill -0 "$PID" 2>/dev/null; then
    echo "WARNING: QEMU session timed out after ${SESSION_TIMEOUT}s"
    kill "$PID" 2>/dev/null || true
    timed_out=true
fi
wait "$PID" 2>/dev/null
qemu_status=$?

# Step 5: Parse and report results
echo ""
echo "========================================"
echo "  Zircon Core Test Results"
echo "========================================"

# Show the raw output (last 50 lines)
tail -50 "$OUTPUT"
echo ""

# Check for failures before cleaning up temp files
test_failed=false
if grep -q 'FAILED' "$OUTPUT" 2>/dev/null; then
    test_failed=true
fi

rm -rf "$TMPDIR_QEMU"

# Exit with failure if tests failed or timed out
if $timed_out; then
    echo "FAILED: test session timed out."
    exit 1
fi
if $test_failed; then
    echo "FAILED: some tests did not pass."
    exit 1
fi
if [ "$qemu_status" -ne 0 ]; then
    echo "FAILED: QEMU exited with status $qemu_status."
    exit "$qemu_status"
fi
echo "Done."
