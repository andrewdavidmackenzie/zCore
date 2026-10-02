#!/bin/bash
# Build an ext4 boot image and write it to the Jolla C2 SD card.
# Usage: ./make-sdcard-image.sh /dev/diskN
set -e

DISK="${1:?Usage: $0 /dev/diskN}"
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

KERNEL="$ROOT/target/jolla-c2/release/kernel.bin"
DTB="$ROOT/prebuilt/jolla-c2/ums9230-reeder-s19mps.dtb"
EXTLINUX="$ROOT/tools/jolla-c2/extlinux.conf"
IMG="$ROOT/target/jolla-c2/release/sdcard-boot.img"

if [ ! -f "$KERNEL" ]; then
    echo "Error: kernel not found at $KERNEL -- run 'make jollac2-build' first" >&2
    exit 1
fi
if [ ! -f "$DTB" ]; then
    echo "Error: DTB not found at $DTB" >&2
    exit 1
fi

# Reject internal disks
if diskutil info "$DISK" 2>/dev/null | grep -q "Internal:.*Yes"; then
    echo "Error: $DISK is an internal disk. Refusing to write." >&2
    exit 1
fi

# Show target device and confirm
echo "==> Target device:"
diskutil info "$DISK" 2>/dev/null | grep -E "Device|Media Name|Total Size|Internal" || true
echo ""
echo "WARNING: This will overwrite the boot partition on $DISK"
echo "Type 'yes' to continue:"
read -r answer
if [ "$answer" != "yes" ]; then
    echo "Aborting." >&2
    exit 1
fi

echo "Creating ext4 boot image..."
dd if=/dev/zero of="$IMG" bs=1M count=32 2>/dev/null
mkfs.ext4 -q -L zcore-boot "$IMG"

echo "Writing files into image..."
debugfs -w "$IMG" <<EOF
mkdir extlinux
write "$EXTLINUX" extlinux/extlinux.conf
write "$KERNEL" zcore.bin
write "$DTB" ums9230-reeder-s19mps.dtb
EOF

# Verify files were written
debugfs -R "stat zcore.bin" "$IMG" >/dev/null 2>&1 || { echo "Error: kernel not written" >&2; exit 1; }
debugfs -R "stat ums9230-reeder-s19mps.dtb" "$IMG" >/dev/null 2>&1 || { echo "Error: DTB not written" >&2; exit 1; }

echo "==> Writing to ${DISK}s1..."
sudo diskutil unmountDisk force "$DISK" 2>/dev/null || true
sudo dd if="$IMG" of="${DISK}s1" bs=4096 2>&1

echo "==> Ejecting $DISK..."
diskutil eject "$DISK"
echo "==> Done. Insert SD card into Jolla C2 and power on."
