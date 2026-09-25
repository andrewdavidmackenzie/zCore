#!/usr/bin/env bash
#
# Build custom pftf/RPi4 UEFI firmware (RPI_EFI.fd) with Zirconia boot logo.
#
# This script:
#   1. Clones pftf/RPi4 at the pinned version (with edk2 submodules)
#   2. Replaces the Raspberry Pi logo BMP with the Zirconia crystal logo
#   3. Optionally patches the firmware vendor/version strings
#   4. Builds RPI_EFI.fd via the EDK2 build system
#   5. Copies the result to target/pftf-firmware/RPI_EFI.fd
#
# The build requires an aarch64 cross-compiler and runs natively on Linux
# or inside Docker on macOS.
#
# Prerequisites (native Linux):
#   - gcc-aarch64-linux-gnu
#   - acpica-tools (iasl)
#   - uuid-dev
#   - make, python3
#
# Prerequisites (macOS via Docker):
#   - Docker Desktop
#
# Usage:
#   tools/scripts/build-uefi-firmware.sh          # auto-detect (Docker on macOS)
#   tools/scripts/build-uefi-firmware.sh --docker  # force Docker
#   tools/scripts/build-uefi-firmware.sh --native  # force native (Linux only)

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/../.." && pwd)"

PFTF_VERSION="v1.53"
PFTF_BUILD_DIR="$PROJECT_DIR/target/pftf-build"
OUTPUT_DIR="$PROJECT_DIR/target/pftf-firmware"
LOGO_BMP="$PROJECT_DIR/assets/images/zirconia-boot-logo.bmp"

# Firmware vendor and version strings shown in UEFI setup menu
FW_VENDOR="https://github.com/andrewdavidmackenzie/zCore"
FW_VERSION="Zirconia UEFI ${PFTF_VERSION}"

# ── Parse arguments ────────────────────────────────────────────────────
MODE=""
for arg in "$@"; do
    case "$arg" in
        --docker) MODE="docker" ;;
        --native) MODE="native" ;;
        --help|-h)
            echo "Usage: $0 [--docker|--native]"
            echo "  --docker  Force Docker build (works on macOS and Linux)"
            echo "  --native  Force native build (Linux with cross-compiler)"
            echo "  (default) Auto-detect: Docker on macOS, native on Linux"
            exit 0
            ;;
        *) echo "Unknown argument: $arg"; exit 1 ;;
    esac
done

# Auto-detect mode
if [ -z "$MODE" ]; then
    if [ "$(uname -s)" = "Darwin" ]; then
        MODE="docker"
    else
        MODE="native"
    fi
fi

# ── Verify logo BMP exists ────────────────────────────────────────────
if [ ! -f "$LOGO_BMP" ]; then
    echo "ERROR: Logo BMP not found: $LOGO_BMP"
    echo "Run: tools/scripts/generate-boot-logo.sh"
    exit 1
fi

# ── Clone pftf/RPi4 (if needed) ───────────────────────────────────────
# EDK2 has deeply nested submodules (OpenSSL -> BoringSSL, etc.) that are
# huge and unnecessary for the firmware build.  We clone the top-level
# pftf repo, then init only the three required submodules (edk2,
# edk2-platforms, edk2-non-osi) with shallow depth.  Inside edk2 we
# selectively init only the submodules the RPi4 platform actually needs.
if [ ! -d "$PFTF_BUILD_DIR/.git" ]; then
    echo "==> Cloning pftf/RPi4 ($PFTF_VERSION)..."
    mkdir -p "$(dirname "$PFTF_BUILD_DIR")"
    git clone --depth 1 --branch "$PFTF_VERSION" \
        https://github.com/pftf/RPi4.git "$PFTF_BUILD_DIR"

    cd "$PFTF_BUILD_DIR"

    # Init the three top-level submodules (shallow)
    git submodule update --init --depth 1 edk2 edk2-platforms edk2-non-osi

    # Inside edk2, init the submodules required for an RPi4 RELEASE build.
    # We use --init on all registered submodules but only --depth 1 to keep
    # the checkout small.  This is simpler than cherry-picking paths and
    # avoids breakage when EDK2 adds new package-level include paths.
    cd edk2
    git submodule update --init --depth 1

    # Some submodules have their own nested submodules that are required.
    cd CryptoPkg/Library/MbedTlsLib/mbedtls
    git submodule update --init --depth 1 framework
    cd "$PFTF_BUILD_DIR/edk2"
    cd SecurityPkg/DeviceSecurity/SpdmLib/libspdm
    git submodule update --init --depth 1 \
        os_stub/mbedtlslib/mbedtls \
        os_stub/openssllib/openssl
    cd os_stub/mbedtlslib/mbedtls
    git submodule update --init --depth 1 framework
    cd "$PFTF_BUILD_DIR"

    echo "  Submodules ready."
else
    echo "==> Using existing pftf/RPi4 checkout at $PFTF_BUILD_DIR"
fi

# ── Replace logo ───────────────────────────────────────────────────────
LOGO_DST="$PFTF_BUILD_DIR/edk2-non-osi/Platform/RaspberryPi/Drivers/LogoDxe/Logo.bmp"
if [ ! -f "$LOGO_DST" ]; then
    echo "ERROR: EDK2 logo path not found: $LOGO_DST"
    echo "The pftf checkout may be incomplete. Try: rm -rf $PFTF_BUILD_DIR"
    exit 1
fi

echo "==> Replacing RPi boot logo with Zirconia logo..."
cp "$LOGO_BMP" "$LOGO_DST"

# ── Build ──────────────────────────────────────────────────────────────
mkdir -p "$OUTPUT_DIR"

build_native() {
    echo "==> Building EDK2 firmware (native)..."

    # Verify cross-compiler
    if ! command -v aarch64-linux-gnu-gcc >/dev/null 2>&1; then
        echo "ERROR: aarch64-linux-gnu-gcc not found."
        echo "Install: sudo apt install gcc-aarch64-linux-gnu acpica-tools uuid-dev"
        exit 1
    fi

    cd "$PFTF_BUILD_DIR"

    # Build EDK2 BaseTools (host-native Python/C tools)
    make -C edk2/BaseTools -j"$(nproc)"

    # Set up EDK2 environment
    export WORKSPACE="$PFTF_BUILD_DIR"
    export PACKAGES_PATH="$WORKSPACE/edk2:$WORKSPACE/edk2-platforms:$WORKSPACE/edk2-non-osi"
    export GCC_AARCH64_PREFIX=aarch64-linux-gnu-

    # shellcheck source=/dev/null
    source edk2/edksetup.sh

    # Build RELEASE firmware
    # - Boot timeout 0: skip "ESC/F1/ENTER" prompt, boot immediately
    # - Keep network stack for future PXE boot support
    # - Disable iSCSI, TLS, Secure Boot (not needed, saves boot time)
    build -a AARCH64 -t GCC -b RELEASE \
        -p edk2-platforms/Platform/RaspberryPi/RPi4/RPi4.dsc \
        --pcd "gEfiMdeModulePkgTokenSpaceGuid.PcdFirmwareVendor=L\"${FW_VENDOR}\"" \
        --pcd "gEfiMdeModulePkgTokenSpaceGuid.PcdFirmwareVersionString=L\"${FW_VERSION}\"" \
        --pcd "gEfiMdePkgTokenSpaceGuid.PcdPlatformBootTimeOut=0" \
        -D SECURE_BOOT_ENABLE=FALSE \
        -D INCLUDE_TFTP_COMMAND=TRUE \
        -D NETWORK_ISCSI_ENABLE=FALSE \
        -D NETWORK_TLS_ENABLE=FALSE \
        -D NETWORK_ALLOW_HTTP_CONNECTIONS=TRUE \
        -D SMC_PCI_SUPPORT=1

    # Copy result
    cp Build/RPi4/RELEASE_GCC/FV/RPI_EFI.fd "$OUTPUT_DIR/RPI_EFI.fd"
}

build_docker() {
    echo "==> Building EDK2 firmware (Docker)..."

    if ! command -v docker >/dev/null 2>&1; then
        echo "ERROR: Docker not found. Install Docker Desktop."
        exit 1
    fi

    # Write the inner build script to a temp file to avoid shell quoting
    # issues with PCD string values passed through docker -> bash -c.
    INNER_SCRIPT="$PFTF_BUILD_DIR/.docker-build.sh"
    cat > "$INNER_SCRIPT" << 'DOCKER_SCRIPT'
#!/bin/bash
set -eo pipefail

# Install build dependencies
apt-get update -qq
apt-get install -y -qq \
    gcc-aarch64-linux-gnu acpica-tools uuid-dev \
    make python3 python-is-python3 gcc g++ >/dev/null 2>&1

cd /build
export WORKSPACE=/build
export PACKAGES_PATH="/build/edk2:/build/edk2-platforms:/build/edk2-non-osi"
export GCC_AARCH64_PREFIX=aarch64-linux-gnu-
export PYTHON_COMMAND=python3

# Build BaseTools
echo "==> Building EDK2 BaseTools..."
make -C edk2/BaseTools -j"$(nproc)" >/dev/null 2>&1

# Set up environment (edksetup.sh uses PYTHON_COMMAND)
source edk2/edksetup.sh

# Build firmware
# - Boot timeout 0: skip "ESC/F1/ENTER" prompt, boot immediately
# - Keep network stack for future PXE boot support
# - Disable iSCSI, TLS, Secure Boot (not needed, saves boot time)
echo "==> Building AARCH64 RELEASE firmware..."
build -a AARCH64 -t GCC -b RELEASE \
    -p edk2-platforms/Platform/RaspberryPi/RPi4/RPi4.dsc \
    --pcd gEfiMdeModulePkgTokenSpaceGuid.PcdFirmwareVendor=L"$FW_VENDOR" \
    --pcd gEfiMdeModulePkgTokenSpaceGuid.PcdFirmwareVersionString=L"$FW_VERSION" \
    --pcd gEfiMdePkgTokenSpaceGuid.PcdPlatformBootTimeOut=0 \
    -D SECURE_BOOT_ENABLE=FALSE \
    -D INCLUDE_TFTP_COMMAND=TRUE \
    -D NETWORK_ISCSI_ENABLE=FALSE \
    -D NETWORK_TLS_ENABLE=FALSE \
    -D NETWORK_ALLOW_HTTP_CONNECTIONS=TRUE \
    -D SMC_PCI_SUPPORT=1

# Copy result
cp Build/RPi4/RELEASE_GCC/FV/RPI_EFI.fd /output/RPI_EFI.fd
echo "==> Firmware built successfully."
DOCKER_SCRIPT
    chmod +x "$INNER_SCRIPT"

    # Build inside an Ubuntu container with the cross-compiler.
    docker run --rm \
        -v "$PFTF_BUILD_DIR:/build" \
        -v "$OUTPUT_DIR:/output" \
        -e "FW_VENDOR=$FW_VENDOR" \
        -e "FW_VERSION=$FW_VERSION" \
        ubuntu:22.04 \
        bash /build/.docker-build.sh
}

case "$MODE" in
    native) build_native ;;
    docker) build_docker ;;
esac

# ── Verify output ──────────────────────────────────────────────────────
if [ ! -f "$OUTPUT_DIR/RPI_EFI.fd" ]; then
    echo "ERROR: Build failed — RPI_EFI.fd not found."
    exit 1
fi

FD_SIZE=$(wc -c < "$OUTPUT_DIR/RPI_EFI.fd" | tr -d ' ')
echo ""
echo "==> Custom UEFI firmware built successfully!"
echo "    $OUTPUT_DIR/RPI_EFI.fd ($FD_SIZE bytes)"
echo ""
echo "Next steps:"
echo "  - Run 'make raspi400-uefi-sd SD=/Volumes/boot' to prepare an SD card"
echo "  - Or copy RPI_EFI.fd to an existing Pi 400 UEFI SD card"
