#!/usr/bin/env bash
# Find OVMF UEFI firmware for x86_64 QEMU.
# Source this script or call find_ovmf() to get the path.

find_ovmf() {
    for candidate in \
        /opt/homebrew/Cellar/qemu/*/share/qemu/edk2-x86_64-code.fd \
        /usr/share/OVMF/OVMF_CODE.fd \
        /usr/share/OVMF/OVMF_CODE_4M.fd \
        /usr/share/ovmf/OVMF.fd \
        /usr/share/edk2/x64/OVMF_CODE.fd \
        /usr/share/qemu/edk2-x86_64-code.fd \
        /usr/share/edk2-ovmf/OVMF_CODE.fd; do
        if [ -f "$candidate" ]; then
            echo "$candidate"
            return 0
        fi
    done
    # Last resort: search common locations
    local found
    found=$(find /usr/share -name "OVMF_CODE*.fd" -o -name "OVMF.fd" -o -name "edk2-x86_64-code.fd" 2>/dev/null | head -1)
    if [ -n "$found" ]; then
        echo "$found"
        return 0
    fi
    echo "ERROR: OVMF UEFI firmware not found. Install qemu or ovmf package." >&2
    return 1
}
