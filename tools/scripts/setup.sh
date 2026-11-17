#!/usr/bin/env bash
#
# Install all host dependencies needed to build and test zCore.
#
# Usage: tools/scripts/setup.sh   (or: make setup)
#
# Installs, per platform:
#   - QEMU system emulators (x86_64, aarch64, riscv64)
#   - UEFI firmware: OVMF for x86_64 QEMU boots; aarch64 firmware for
#     the qemu-aarch64-uefi board (optional on CI-less setups)
#   - mtools (FAT/ESP image creation for aarch64 UEFI boots)
#   - e2fsprogs (ext4 boot images for the Jolla C2)
#   - macOS: musl cross-toolchains via Homebrew (make config-macos)
#
# Rust is NOT installed by this script: install rustup yourself
# (https://rustup.rs). The exact nightly toolchain and its components
# are pinned in rust-toolchain.toml and install automatically on the
# first `cargo` invocation. Extra cross-compilation rustup targets
# (musl for demo binaries, aarch64-unknown-uefi for the UEFI stub)
# are added here when rustup is available.
#
# The musl cross-toolchains and busybox sources used for the rootfs
# are downloaded automatically into .build-cache/ on first use
# (Linux); on macOS they come from Homebrew.

set -euo pipefail

PROJECT_DIR="$(cd "$(dirname "$0")/../.." && pwd)"

echo "==> zCore environment setup ($(uname -s) $(uname -m))"

# --- Rust toolchain (required) -------------------------------------------
if ! command -v rustup >/dev/null 2>&1; then
    echo "WARNING: rustup is not installed."
    echo "  Install it from https://rustup.rs, then re-run 'make setup'."
    echo "  The pinned nightly toolchain (rust-toolchain.toml) installs"
    echo "  automatically on the first cargo invocation."
else
    # Run inside the project so rust-toolchain.toml selects the pinned
    # nightly. Best-effort: these targets are optional niceties.
    cd "$PROJECT_DIR"
    for target in aarch64-unknown-linux-musl x86_64-unknown-linux-musl aarch64-unknown-uefi; do
        if rustup target add "$target" 2>/dev/null; then
            echo "  rustup target added: $target"
        else
            echo "  (skipped: rustup target $target)"
        fi
    done
fi

# --- System packages ------------------------------------------------------
OS="$(uname -s)"
case "$OS" in
    Linux)
        if command -v apt-get >/dev/null 2>&1; then
            echo "==> Installing packages via apt (Ubuntu/Debian)..."
            sudo apt-get update
            sudo apt-get install -y \
                qemu-system-x86 qemu-system-arm qemu-system-misc qemu-utils \
                ovmf qemu-efi-aarm64 \
                mtools e2fsprogs
        elif command -v pacman >/dev/null 2>&1; then
            echo "==> Installing packages via pacman (Arch)..."
            sudo pacman -Sy --needed \
                qemu-system-x86 qemu-system-aarch64 qemu-system-riscv \
                edk2-ovmf edk2-aarch64 \
                mtools e2fsprogs
        elif command -v dnf >/dev/null 2>&1; then
            echo "==> Installing packages via dnf (Fedora)..."
            sudo dnf install -y \
                qemu-system-x86-core qemu-system-aarch64-core \
                edk2-ovmf edk2-aarch64 \
                mtools e2fsprogs
        else
            echo "WARNING: unsupported Linux package manager."
            echo "  Install manually: qemu-system-{x86,aarch64,riscv}, UEFI"
            echo "  firmware (OVMF + aarch64), mtools, e2fsprogs."
        fi
        ;;
    Darwin)
        echo "==> Installing packages via Homebrew (macOS)..."
        brew install qemu mtools e2fsprogs
        echo "==> Installing musl cross-toolchains + kernel headers..."
        make -C "$PROJECT_DIR" config-macos
        ;;
    *)
        echo "WARNING: unsupported OS '$OS'. Install QEMU, UEFI firmware,"
        echo "  mtools, and e2fsprogs manually."
        ;;
esac

# --- Verify the essentials ------------------------------------------------
echo "==> Verifying installation..."
MISSING=0
for tool in qemu-system-x86_64 qemu-system-aarch64 cargo; do
    if ! command -v "$tool" >/dev/null 2>&1; then
        echo "  MISSING: $tool"
        MISSING=1
    else
        echo "  ok: $tool"
    fi
done
if [ "$MISSING" -ne 0 ]; then
    echo "Setup incomplete — see the messages above."
    exit 1
fi

echo "==> Setup complete."
echo "    Next: make run CONFIG=busybox.qemu-aarch64   (boots the busybox shell)"
echo "    Or:   cargo zcore set busybox.qemu-aarch64   (persist a default selection)"
