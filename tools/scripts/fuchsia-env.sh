#!/usr/bin/env bash
#
# Shared configuration for running real Fuchsia builds in QEMU.
#
# Sources (does not execute anything by itself). Sets:
#   FUCHSIA_ROOT  - jiri checkout root (contains out/, prebuilt/)
#   FUCHSIA_OUT   - build output dir (contains images.json)
#   FUCHSIA_ZBI   - full boot image (zircon-a ZBI)
#   FUCHSIA_QEMU_KERNEL - QEMU boot shim (qemu-kernel image)
#   FUCHSIA_QEMU_BIN    - qemu-system-x86_64 binary
#   FUCHSIA_KVM   - "1" if KVM acceleration is available
#   FUCHSIA_ENTROPY    - random hex string for kernel.entropy-mixin
#   FUCHSIA_CMDLINE    - kernel command line (serial, entropy, halt-on-panic)
#
# Override defaults via environment: FUCHSIA_ROOT, FUCHSIA_OUT,
# FUCHSIA_QEMU_BIN, FUCHSIA_MEMORY, FUCHSIA_SMP.
#
# The default FUCHSIA_ROOT matches the local checkout layout at
# ../fuchsia/fuchsia/fuchsia (jiri root nested under the git-only mirror).

# Resolve the zCore workspace root (parent of tools/).
ZCORE_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"

FUCHSIA_ROOT="${FUCHSIA_ROOT:-$ZCORE_ROOT/../fuchsia/fuchsia/fuchsia}"
FUCHSIA_OUT="${FUCHSIA_OUT:-$FUCHSIA_ROOT/out/bringup.x64-balanced}"

fuchsia_die() {
    echo "fuchsia: $*" >&2
    exit 1
}

# Resolve the zircon-a ZBI path from images.json (robust across
# product/board renames; mirrors what `fx qemu` consumes).
# Usage: fuchsia_resolve_zbi
fuchsia_resolve_zbi() {
    [ -f "$FUCHSIA_OUT/images.json" ] ||
        fuchsia_die "no images.json in FUCHSIA_OUT='$FUCHSIA_OUT'
  Point FUCHSIA_ROOT/FUCHSIA_OUT at a built Fuchsia tree, e.g.:
    export FUCHSIA_ROOT=/path/to/fuchsia   # jiri checkout root
  or build it there: fx set bringup.x64 && fx build"
    local json
    json="$(python3 - "$FUCHSIA_OUT/images.json" <<'EOF'
import json, sys
images = json.load(open(sys.argv[1]))
zbi = next(
    (img["path"] for img in images
     if img.get("name") == "zircon-a" and img.get("type") == "zbi"),
    None,
)
if not zbi:
    sys.exit("no zircon-a zbi image in images.json")
print(zbi)
EOF
    )" || fuchsia_die "failed to parse images.json"
    FUCHSIA_ZBI="$FUCHSIA_OUT/$json"
    [ -f "$FUCHSIA_ZBI" ] || fuchsia_die "ZBI not found: $FUCHSIA_ZBI"
}

# Resolve the ZBI and the QEMU boot shim. The shim is required by the
# run/test targets: QEMU's -kernel boots the shim, which implements
# the Linux boot protocol and hands the ZBI (-initrd) to the Fuchsia
# kernel. Without it there is no direct-boot path. (Artifact import
# only needs the ZBI — see fuchsia_resolve_zbi.)
# Usage: fuchsia_resolve_images
fuchsia_resolve_images() {
    fuchsia_resolve_zbi
    local shim
    shim="$(python3 - "$FUCHSIA_OUT/images.json" <<'EOF'
import json, sys
images = json.load(open(sys.argv[1]))
print(next(
    (img["path"] for img in images if img.get("name") == "qemu-kernel"),
    "",
))
EOF
    )" || fuchsia_die "failed to parse images.json"
    if [ -z "$shim" ]; then
        fuchsia_die "no qemu-kernel (boot shim) in images.json — this build
  cannot be booted directly by QEMU. Build a full product (e.g.
  fx set bringup.x64 && fx build) or point FUCHSIA_OUT at one."
    fi
    FUCHSIA_QEMU_KERNEL="$FUCHSIA_OUT/$shim"
    [ -f "$FUCHSIA_QEMU_KERNEL" ] || fuchsia_die "boot shim not found: $FUCHSIA_QEMU_KERNEL"
}

# Pick a QEMU binary: FUCHSIA_QEMU_BIN override, else the system one.
fuchsia_resolve_qemu() {
    FUCHSIA_QEMU_BIN="${FUCHSIA_QEMU_BIN:-$(command -v qemu-system-x86_64 || true)}"
    [ -n "$FUCHSIA_QEMU_BIN" ] ||
        fuchsia_die "qemu-system-x86_64 not found; install qemu or set FUCHSIA_QEMU_BIN"
    if [ -w /dev/kvm ]; then
        FUCHSIA_KVM=1
    else
        FUCHSIA_KVM=0
    fi
}

# Random entropy for the kernel CSPRNG (sha256sum on Linux, shasum on macOS).
fuchsia_gen_entropy() {
    if command -v sha256sum >/dev/null 2>&1; then
        FUCHSIA_ENTROPY="$(head -c 32 /dev/urandom | sha256sum | awk '{print $1}')"
    else
        FUCHSIA_ENTROPY="$(head -c 32 /dev/urandom | shasum -a 256 | awk '{print $1}')"
    fi
}

fuchsia_build_cmdline() {
    fuchsia_gen_entropy
    # kernel.serial=legacy : console on the legacy COM1 ioport
    # halt-on-panic        : don't reboot-loop on kernel panic (CI-friendly)
    FUCHSIA_CMDLINE="kernel.serial=legacy kernel.entropy-mixin=$FUCHSIA_ENTROPY kernel.halt-on-panic=true"
}

# Common QEMU arguments (mirrors zircon/scripts/run-zircon x64 defaults).
# Usage: fuchsia_qemu_args
fuchsia_qemu_args() {
    fuchsia_resolve_qemu
    fuchsia_build_cmdline
    FUCHSIA_MEMORY="${FUCHSIA_MEMORY:-8192}"
    FUCHSIA_SMP="${FUCHSIA_SMP:-4}"
    FUCHSIA_QEMU_ARGS=(
        -machine q35,smbios-entry-point-type=32
        -device isa-debug-exit,iobase=0xf4,iosize=0x04
        # Keep SeaBIOS from printing a terminal reset on start.
        -fw_cfg name=etc/sercon-port,string=0
        -m "$FUCHSIA_MEMORY"
        -smp "$FUCHSIA_SMP"
        -kernel "$FUCHSIA_QEMU_KERNEL"
        -initrd "$FUCHSIA_ZBI"
        -no-reboot
    )
    if [ "$FUCHSIA_KVM" = 1 ]; then
        FUCHSIA_QEMU_ARGS+=(-enable-kvm -cpu host,migratable=no,+invtsc)
    else
        FUCHSIA_QEMU_ARGS+=(-cpu Skylake-Client,-check)
    fi
}

# CI trigger marker (runs did not start for c36ce499)
