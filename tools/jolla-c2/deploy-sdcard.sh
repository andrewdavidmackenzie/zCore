#!/bin/bash
#
# Deploy zCore kernel to the SD card for Jolla C2 boot.
#
# Usage:
#   ./deploy-sdcard.sh /dev/diskN /path/to/zcore.bin
#
# This formats the SD card with a single ext4 partition, copies the
# kernel binary, DTB, and extlinux.conf. The SD card can then be
# inserted into the phone (which must already have U-Boot flashed
# to boot_a/boot_b).
#
set -e

if [ $# -lt 2 ]; then
    echo "Usage: $0 <disk-device> <zcore-binary>" >&2
    echo "Example: $0 /dev/disk6 ../../target/jolla-c2/release/kernel.bin" >&2
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

# Reject internal disks
if diskutil info "$DISK" 2>/dev/null | grep -q "Internal:.*Yes"; then
    echo "Error: $DISK is an internal disk. Refusing to write." >&2
    exit 1
fi

echo "=== Deploy zCore to SD card ==="
echo "Disk:    $DISK"
echo "Kernel:  $KERNEL"
echo "DTB:     $DTB"
echo ""
diskutil info "$DISK" 2>/dev/null | grep -E "Device|Media Name|Total Size|Internal" || true
echo ""
echo "WARNING: This will erase all data on $DISK"
echo "Type 'yes' to continue or Ctrl-C to abort:"
read -r answer
if [ "$answer" != "yes" ]; then
    echo "Aborting." >&2
    exit 1
fi

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
MKFS=$(which mkfs.ext4 2>/dev/null || which mke2fs 2>/dev/null)
if [ -z "$MKFS" ]; then
    echo "Error: mkfs.ext4 not found. Install with: brew install e2fsprogs" >&2
    exit 1
fi

echo "Formatting as ext4..."
sudo "$MKFS" -t ext4 -L zcore-boot "${DISK}s1"

# Create ext4 image with files using debugfs, then dd to partition
echo "Creating boot image with kernel, DTB, and extlinux.conf..."
IMG=$(mktemp /tmp/zcore-deploy.XXXXXX.img)
dd if=/dev/zero of="$IMG" bs=1M count=32 2>/dev/null
"$MKFS" -q -t ext4 -L zcore-boot "$IMG"

debugfs -w "$IMG" <<EOF
mkdir extlinux
write "$EXTLINUX" extlinux/extlinux.conf
write "$KERNEL" zcore.bin
write "$DTB" ums9230-reeder-s19mps.dtb
EOF

# Verify DTB was written
debugfs -R "stat ums9230-reeder-s19mps.dtb" "$IMG" >/dev/null 2>&1 || {
    echo "Error: DTB not written to image" >&2
    rm "$IMG"
    exit 1
}

echo "Writing image to ${DISK}s1..."
sudo dd if="$IMG" of="${DISK}s1" bs=4096 2>&1
rm "$IMG"

echo ""
echo "Done. Eject the SD card:"
echo "  diskutil eject $DISK"
echo ""
echo "Insert into the phone and power on."
