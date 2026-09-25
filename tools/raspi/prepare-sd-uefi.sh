#!/usr/bin/env bash
#
# Prepare a microSD card for UEFI-booting zCore on Raspberry Pi 400.
#
# This script:
# 1. Downloads pftf/RPi4 UEFI firmware (if not cached)
# 2. Builds the UEFI stub and kernel for raspi400-uefi
# 3. Creates an ESP-compatible layout on the SD card
#
# Usage:
#   tools/raspi/prepare-sd-uefi.sh /Volumes/boot
#   tools/raspi/prepare-sd-uefi.sh /mnt/boot
#
# Prerequisites:
#   - A microSD card with a FAT32 partition (acts as ESP), mounted
#   - Rust toolchain with aarch64 support
#   - rust-objcopy (from cargo-binutils)
#   - curl, unzip
#
# SD card layout (FAT32 / ESP):
#   /                          -- Pi firmware + pftf UEFI files
#   /EFI/BOOT/BOOTAA64.EFI    -- UEFI stub (zCore boot loader)
#   /kernel                    -- zCore kernel ELF
#   /initrd.img                -- rootfs initrd (optional)
#   /bcm2711-rpi-400.dtb       -- DTB (from pftf release)
#
# The pftf firmware replaces the Pi's start4.elf boot chain with
# a standard UEFI implementation (EDK2). The Pi GPU loads the pftf
# RPI_EFI.fd as the "kernel", which then provides UEFI services.
# Our UEFI stub is installed as the default UEFI boot application.

set -euo pipefail

BOOT_DIR="${1:?Usage: $0 <mount-point-of-SD-FAT32-partition>}"
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/../.." && pwd)"
PFTF_CACHE="$PROJECT_DIR/target/pftf-firmware"
PFTF_VERSION="v1.53"  # Stable pftf release (2026-08-31)

if [ ! -d "$BOOT_DIR" ]; then
    echo "ERROR: $BOOT_DIR does not exist"
    exit 1
fi

# Verify the path is a mount point (not just a local directory)
if command -v mountpoint >/dev/null 2>&1; then
    if ! mountpoint -q "$BOOT_DIR"; then
        echo "WARNING: $BOOT_DIR does not appear to be a mount point."
        echo "         If this is not a mounted SD card partition, files will"
        echo "         be written to the host filesystem instead."
        read -r -p "Continue anyway? [y/N] " reply
        if [ "$reply" != "y" ] && [ "$reply" != "Y" ]; then
            exit 1
        fi
    fi
fi

echo "==> Preparing Pi 400 UEFI SD card at: $BOOT_DIR"

# --- Step 1: Download pftf/RPi4 UEFI firmware ---
if [ ! -f "$PFTF_CACHE/RPI_EFI.fd" ]; then
    echo "==> Downloading pftf/RPi4 UEFI firmware ($PFTF_VERSION)..."
    mkdir -p "$PFTF_CACHE"

    PFTF_URL="https://github.com/pftf/RPi4/releases/download/${PFTF_VERSION}/RPi4_UEFI_Firmware_${PFTF_VERSION}.zip"
    curl --fail -sL "$PFTF_URL" -o "$PFTF_CACHE/pftf.zip"
    (cd "$PFTF_CACHE" && unzip -o pftf.zip)
    rm -f "$PFTF_CACHE/pftf.zip"
    echo "  Firmware cached at $PFTF_CACHE"
else
    echo "==> Using cached pftf firmware at $PFTF_CACHE"
fi

# --- Step 2: Build the UEFI stub ---
echo "==> Building UEFI stub for Pi 400..."
cd "$PROJECT_DIR/tools/aarch64-uefi-stub"
cargo build --release --target aarch64-unknown-uefi --features board-raspi400

STUB_EFI="$PROJECT_DIR/tools/aarch64-uefi-stub/target/aarch64-unknown-uefi/release/aarch64-uefi-stub.efi"
if [ ! -f "$STUB_EFI" ]; then
    echo "ERROR: UEFI stub not found at $STUB_EFI"
    exit 1
fi

# --- Step 3: Build the kernel ---
echo "==> Building zCore kernel for Pi 400 UEFI..."
cd "$PROJECT_DIR"
cargo xtask zcore-build -m raspi400-uefi

KERNEL_ELF="$PROJECT_DIR/target/raspi400-uefi/release/kernel"
if [ ! -f "$KERNEL_ELF" ]; then
    echo "ERROR: Kernel ELF not found at $KERNEL_ELF"
    exit 1
fi

# --- Step 4: Remove old raw boot files that conflict with UEFI boot ---
echo "==> Removing old raw boot files (if any)..."
for f in kernel8.img armstub8-gic.bin cmdline.txt; do
    if [ -f "$BOOT_DIR/$f" ]; then
        echo "  Removing $f (raw boot leftover)"
        rm -f "$BOOT_DIR/$f"
    fi
done
# Remove old config.txt — pftf provides its own
rm -f "$BOOT_DIR/config.txt"
# Remove old firmware backups
rm -f "$BOOT_DIR/start4.elf.old" "$BOOT_DIR/fixup4.dat.old"

# --- Step 5: Copy pftf firmware to SD card ---
echo "==> Copying pftf UEFI firmware to $BOOT_DIR..."

# Core pftf files that replace Pi's native boot chain
for f in RPI_EFI.fd config.txt fixup4.dat start4.elf bcm2711-rpi-400.dtb; do
    if [ -f "$PFTF_CACHE/$f" ]; then
        cp "$PFTF_CACHE/$f" "$BOOT_DIR/"
    fi
done

# Copy overlays directory if present
if [ -d "$PFTF_CACHE/overlays" ]; then
    mkdir -p "$BOOT_DIR/overlays"
    cp -r "$PFTF_CACHE/overlays/"* "$BOOT_DIR/overlays/" 2>/dev/null || true
fi

# --- Step 6: Install zCore UEFI stub as default boot application ---
echo "==> Installing zCore UEFI stub..."
mkdir -p "$BOOT_DIR/EFI/BOOT"
cp "$STUB_EFI" "$BOOT_DIR/EFI/BOOT/BOOTAA64.EFI"

# --- Step 7: Copy kernel and initrd ---
echo "==> Copying kernel..."
cp "$KERNEL_ELF" "$BOOT_DIR/kernel"

# Rootfs (initrd)
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
echo "==> Pi 400 UEFI SD card prepared successfully!"
echo ""
echo "Files on $BOOT_DIR:"
ls -la "$BOOT_DIR/"
echo ""
echo "SD card layout:"
echo "  /RPI_EFI.fd              -- pftf UEFI firmware"
echo "  /config.txt              -- Pi GPU boot config (loads RPI_EFI.fd)"
echo "  /start4.elf, fixup4.dat  -- Pi GPU firmware"
echo "  /EFI/BOOT/BOOTAA64.EFI  -- zCore UEFI stub"
echo "  /kernel                  -- zCore kernel ELF"
echo "  /initrd.img              -- rootfs (if built)"
echo ""
echo "Next steps:"
echo "  1. Eject the SD card safely"
echo "  2. Insert into Pi 400"
echo "  3. Connect serial console (GPIO 14/15 or USB-serial)"
echo "  4. Power on -- pftf UEFI will start, then auto-boot zCore"
echo ""
echo "To enter UEFI setup: press ESC during the Pi logo splash screen."
echo "UEFI boot order can be configured from the setup menu."
