#!/usr/bin/env bash
#
# Prepare a microSD card for booting zCore on Raspberry Pi 4 / Pi 400.
#
# This script:
# 1. Downloads Pi firmware files (if not cached)
# 2. Builds the kernel for raspi400
# 3. Copies firmware + kernel + config.txt to an existing FAT32 partition
#
# Usage:
#   tools/raspi/prepare-sd.sh /Volumes/boot     # macOS (SD card mounted at /Volumes/boot)
#   tools/raspi/prepare-sd.sh /mnt/boot          # Linux (SD card mounted at /mnt/boot)
#
# Prerequisites:
#   - A microSD card with a FAT32 partition, mounted at the given path
#   - Rust toolchain with aarch64 support
#   - rust-objcopy (from cargo-binutils)
#   - curl (for downloading firmware)
#
# The SD card should have been formatted with a single FAT32 partition.
# On macOS: Disk Utility -> Erase -> Format: MS-DOS (FAT32)
# On Linux: sudo mkfs.vfat /dev/sdX1

set -euo pipefail

BOOT_DIR="${1:?Usage: $0 <mount-point-of-SD-FAT32-partition>}"
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/../.." && pwd)"
FIRMWARE_CACHE="$PROJECT_DIR/target/raspi-firmware"
FIRMWARE_TAG="1.20241126"  # Stable firmware release

if [ ! -d "$BOOT_DIR" ]; then
    echo "ERROR: $BOOT_DIR does not exist or is not mounted"
    exit 1
fi

echo "==> Preparing SD card at: $BOOT_DIR"

# --- Step 1: Download Pi firmware (cached) ---
if [ ! -f "$FIRMWARE_CACHE/start4.elf" ]; then
    echo "==> Downloading Raspberry Pi firmware ($FIRMWARE_TAG)..."
    mkdir -p "$FIRMWARE_CACHE"

    FIRMWARE_URL="https://github.com/raspberrypi/firmware/raw/$FIRMWARE_TAG/boot"
    for f in start4.elf fixup4.dat bcm2711-rpi-4-b.dtb bcm2711-rpi-400.dtb; do
        echo "  Downloading $f..."
        curl --fail -sL "$FIRMWARE_URL/$f" -o "$FIRMWARE_CACHE/$f"
    done
    echo "  Firmware cached at $FIRMWARE_CACHE"
else
    echo "==> Using cached firmware at $FIRMWARE_CACHE"
fi

# --- Step 2: Build the kernel ---
echo "==> Building zCore kernel for Raspberry Pi 4..."
cd "$PROJECT_DIR"
make raspi400-build

KERNEL_BIN="$PROJECT_DIR/target/raspi400/release/kernel.bin"
if [ ! -f "$KERNEL_BIN" ]; then
    echo "ERROR: Kernel binary not found at $KERNEL_BIN"
    exit 1
fi

# --- Step 3: Copy files to SD card ---
echo "==> Copying files to $BOOT_DIR..."

# Firmware
cp "$FIRMWARE_CACHE/start4.elf" "$BOOT_DIR/"
cp "$FIRMWARE_CACHE/fixup4.dat" "$BOOT_DIR/"

# Device tree blobs (Pi firmware needs these)
cp "$FIRMWARE_CACHE/bcm2711-rpi-4-b.dtb" "$BOOT_DIR/"
cp "$FIRMWARE_CACHE/bcm2711-rpi-400.dtb" "$BOOT_DIR/"

# Create overlays directory (needed for dtoverlay=disable-bt)
mkdir -p "$BOOT_DIR/overlays"
# The disable-bt overlay is in the firmware repo
if [ ! -f "$FIRMWARE_CACHE/disable-bt.dtbo" ]; then
    OVERLAY_URL="https://github.com/raspberrypi/firmware/raw/$FIRMWARE_TAG/boot/overlays/disable-bt.dtbo"
    curl --fail -sL "$OVERLAY_URL" -o "$FIRMWARE_CACHE/disable-bt.dtbo"
fi
cp "$FIRMWARE_CACHE/disable-bt.dtbo" "$BOOT_DIR/overlays/"

# Boot config
cp "$SCRIPT_DIR/config.txt" "$BOOT_DIR/"

# Kernel
cp "$KERNEL_BIN" "$BOOT_DIR/kernel8.img"

# Rootfs (initrd) — contains busybox, petal binaries, etc.
ROOTFS_IMG="$PROJECT_DIR/target/qemu-aarch64/release/aarch64-linux.img"
if [ ! -f "$ROOTFS_IMG" ]; then
    echo "==> Building rootfs image..."
    cd "$PROJECT_DIR"
    cargo image --arch aarch64
fi
if [ -f "$ROOTFS_IMG" ]; then
    cp "$ROOTFS_IMG" "$BOOT_DIR/initrd.img"
    echo "  Copied rootfs ($(du -h "$ROOTFS_IMG" | cut -f1)) as initrd.img"
else
    echo "  WARNING: rootfs image not found, skipping initrd"
fi

echo ""
echo "==> SD card prepared successfully!"
echo ""
echo "Files on $BOOT_DIR:"
ls -la "$BOOT_DIR/"
echo ""
echo "Next steps:"
echo "  1. Eject the SD card safely"
echo "  2. Insert into Pi 400"
echo "  3. Connect a USB-to-serial adapter to GPIO pins 14 (TX) and 15 (RX)"
echo "     Or use the Pi 400's built-in USB and a serial terminal"
echo "  4. Open a serial terminal: screen /dev/tty.usbserial-* 115200"
echo "  5. Power on the Pi"
echo ""
echo "You should see zCore kernel output on the serial console."
echo "The kernel will boot and initialize, but will panic without a rootfs."
