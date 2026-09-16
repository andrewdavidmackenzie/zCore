#!/usr/bin/env bash
#
# Build a UEFI-bootable disk image for x86_64 real hardware.
#
# Requires the vendor/bootloader git submodule to be checked out.
#
# Usage: tools/scripts/x86-uefi-image.sh <output-image>
#
# The output image can be written to a USB drive with dd.

set -euo pipefail

OUTPUT="${1:?Usage: $0 <output-image>}"
KERNEL_ELF="target/x86_64/release/zcore"
ROOTFS_IMG="zCore/x86_64.img"
BOOTIMAGE_DIR="tools/x86-bootimage"
BOOTIMAGE_TOML="$BOOTIMAGE_DIR/Cargo.toml"
BOOTIMAGE_TOOL="$BOOTIMAGE_DIR/target/release/x86-bootimage"

# Check prerequisites
if [ ! -d "vendor/bootloader/uefi" ]; then
    echo "ERROR: vendor/bootloader submodule not checked out."
    echo "Run: git submodule update --init vendor/bootloader"
    exit 1
fi

# Build kernel if needed
if [ ! -f "$KERNEL_ELF" ]; then
    echo "Building x86_64 kernel..."
    cargo image --arch x86_64
    cargo bin -m virt-x86_64
fi

if [ ! -f "$KERNEL_ELF" ]; then
    echo "ERROR: Kernel ELF not found at $KERNEL_ELF"
    echo "Run: make build ARCH=x86_64"
    exit 1
fi

# Save original Cargo.toml and swap to the UEFI-enabled fork
ORIGINAL_DEP=$(grep '^bootloader = ' "$BOOTIMAGE_TOML")
UEFI_DEP='bootloader = { path = "../../vendor/bootloader", default-features = false, features = ["bios", "uefi"] }'

cleanup() {
    # Restore original Cargo.toml
    if [ -f "${BOOTIMAGE_TOML}.orig" ]; then
        mv "${BOOTIMAGE_TOML}.orig" "$BOOTIMAGE_TOML"
    fi
}
trap cleanup EXIT

cp "$BOOTIMAGE_TOML" "${BOOTIMAGE_TOML}.orig"
sed "s|^bootloader = .*|${UEFI_DEP}|" "${BOOTIMAGE_TOML}.orig" > "$BOOTIMAGE_TOML"

echo "Building x86-bootimage tool with UEFI support..."
cargo -Z bindeps build --release --manifest-path "$BOOTIMAGE_TOML" --features uefi

# Build the UEFI image
ARGS=("$KERNEL_ELF" "$OUTPUT" "--uefi")
if [ -f "$ROOTFS_IMG" ]; then
    ARGS+=("--ramdisk" "$ROOTFS_IMG")
fi

"$BOOTIMAGE_TOOL" "${ARGS[@]}"

SIZE=$(stat -f%z "$OUTPUT" 2>/dev/null || stat --printf="%s" "$OUTPUT" 2>/dev/null || echo "?")
echo ""
echo "UEFI boot image: $OUTPUT ($SIZE bytes)"
echo ""
echo "To write to a USB drive:"
echo "  # Find the device"
echo "  diskutil list     # macOS"
echo "  lsblk             # Linux"
echo ""
echo "  # Write the image (WARNING: destroys all data on the device)"
echo "  # macOS:"
echo "  diskutil unmountDisk /dev/diskN"
echo "  sudo dd if=$OUTPUT of=/dev/rdiskN bs=1m"
echo "  # Linux:"
echo "  sudo dd if=$OUTPUT of=/dev/sdX bs=1M status=progress"
echo "  sync"
