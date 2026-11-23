#!/usr/bin/env bash
#
# Run the real Fuchsia kernel + userspace (bringup) in QEMU, with the
# serial console on stdio. This is the interactive demo/baseline for
# issue #523 Phase A: it boots the *Fuchsia* kernel (not zCore) from
# the prebuilt images in the local Fuchsia checkout.
#
# Usage: tools/scripts/fuchsia-run.sh
#
# Environment (see fuchsia-env.sh): FUCHSIA_ROOT, FUCHSIA_OUT,
# FUCHSIA_QEMU_BIN, FUCHSIA_MEMORY, FUCHSIA_SMP.
#
# Exit: QEMU's exit status. Ctrl-C or the QEMU monitor (Ctrl-A x) quits.

set -euo pipefail

source "$(dirname "$0")/fuchsia-env.sh"

fuchsia_resolve_images
fuchsia_qemu_args

echo "fuchsia-run: FUCHSIA_OUT=$FUCHSIA_OUT"
echo "fuchsia-run: ZBI=$FUCHSIA_ZBI"
echo "fuchsia-run: shim=$FUCHSIA_QEMU_KERNEL"
echo "fuchsia-run: KVM=$FUCHSIA_KVM memory=${FUCHSIA_MEMORY}M smp=$FUCHSIA_SMP"
echo "fuchsia-run: serial console on stdio (Ctrl-A x to quit QEMU)"
echo

exec "$FUCHSIA_QEMU_BIN" "${FUCHSIA_QEMU_ARGS[@]}" \
    -display none -vga none \
    -serial mon:stdio \
    -append "$FUCHSIA_CMDLINE" "$@"
