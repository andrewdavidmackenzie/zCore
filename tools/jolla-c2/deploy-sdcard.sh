#!/bin/bash
#
# Deploy zCore kernel to the SD card for Jolla C2 boot.
#
# Usage:
#   ./deploy-sdcard.sh /dev/disk6 /path/to/zcore.bin
#
# This formats the SD card with a single ext4 partition, copies the
# kernel binary, DTB, and extlinux.conf. The SD card can then be
# inserted into the phone (which must already have U-Boot flashed
# to boot_a/boot_b).
#
set -e

if [ $# -lt 2 ]; then
    echo "Usage: $0 <disk-device> <zcore-binary>" >&2
    echo "Example: $0 /dev/disk6 ../../zcore.bin" >&2
    exit 1
fi

DISK="$1"
KERNEL="$2"
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
WORKSPACE_ROOT="${SCRIPT_DIR}/../.."
DTB="${WORKSPACE_ROOT}/prebuilt/jolla-c2/ums9230-reeder-s19mps.dtb"
EXTLINUX="${SCRIPT_DIR}/extlinux.conf"

if [ ! -f "$KERNEL" ]; then
    echo "Error: kernel binary not found: $KERNEL" >&2
    exit 1
fi

if [ ! -f "$DTB" ]; then
    echo "Error: DTB not found: $DTB" >&2
    exit 1
fi

echo "=== Deploy zCore to SD card ==="
echo "Disk:    $DISK"
echo "Kernel:  $KERNEL"
echo "DTB:     $DTB"
echo ""
echo "WARNING: This will erase all data on $DISK"
echo "Press Enter to continue or Ctrl-C to abort..."
read

# Unmount
diskutil unmountDisk "$DISK" 2>/dev/null || true

# Create GPT with single boot partition
echo "Partitioning..."
sudo sgdisk --zap-all "$DISK"
sudo sgdisk \
    --new=1:0:0 \
    --typecode=1:8300 \
    --change-name=1:"zcore-boot" \
    "$DISK"

# Wait for partition to appear
sleep 2

# Format as ext4
# macOS doesn't have mkfs.ext4 by default — use the one from e2fsprogs
MKFS=$(which mkfs.ext4 2>/dev/null || which mke2fs 2>/dev/null)
if [ -z "$MKFS" ]; then
    echo "Error: mkfs.ext4 not found. Install with: brew install e2fsprogs" >&2
    exit 1
fi

echo "Formatting as ext4..."
sudo "$MKFS" -t ext4 -L zcore-boot "${DISK}s1"

# Mount
MOUNT_DIR=$(mktemp -d)
echo "Mounting at $MOUNT_DIR..."

# On macOS, ext4 mounting requires ext4fuse or similar
# Fallback: use debugfs to write files, or write a raw image
if command -v ext4fuse &>/dev/null; then
    ext4fuse "${DISK}s1" "$MOUNT_DIR"
    MOUNTED=1
else
    echo ""
    echo "Cannot mount ext4 on macOS without ext4fuse."
    echo "Alternative: create an ext4 image file and dd it."
    echo ""
    echo "Creating ext4 image with embedded files..."

    IMG=$(mktemp /tmp/zcore-boot.XXXXXX.img)
    # 32 MB image is plenty for kernel + DTB + extlinux.conf
    dd if=/dev/zero of="$IMG" bs=1M count=32
    "$MKFS" -t ext4 -L zcore-boot "$IMG"

    # Use debugfs to copy files into the image
    debugfs -w -R "mkdir extlinux" "$IMG"
    debugfs -w -R "write $EXTLINUX extlinux/extlinux.conf" "$IMG"
    debugfs -w -R "write $KERNEL zcore.bin" "$IMG"
    debugfs -w -R "write $DTB ums9230-reeder-s19mps.dtb" "$IMG"

    echo "Writing image to ${DISK}s1..."
    sudo dd if="$IMG" of="${DISK}s1" bs=4096
    rm "$IMG"

    MOUNTED=0
fi

if [ "$MOUNTED" = 1 ]; then
    mkdir -p "$MOUNT_DIR/extlinux"
    cp "$EXTLINUX" "$MOUNT_DIR/extlinux/extlinux.conf"
    cp "$KERNEL" "$MOUNT_DIR/zcore.bin"
    cp "$DTB" "$MOUNT_DIR/ums9230-reeder-s19mps.dtb"
    umount "$MOUNT_DIR"
fi

rmdir "$MOUNT_DIR" 2>/dev/null || true

echo ""
echo "Done. Eject the SD card:"
echo "  diskutil eject $DISK"
echo ""
echo "Insert into the phone and power on."
