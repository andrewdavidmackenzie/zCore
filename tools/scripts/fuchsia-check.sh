#!/usr/bin/env bash
#
# Doctor for the Fuchsia baseline (issue #523 Phase A): verify the
# Fuchsia tree exists and is built, and print the artifact inventory
# that fuchsia-run/fuchsia-test consume.
#
# Usage: tools/scripts/fuchsia-check.sh
#
# Exit code 0 = tree and artifacts look usable
# Exit code 1 = missing tree/build/artifacts (message explains what to do)

set -euo pipefail

source "$(dirname "$0")/fuchsia-env.sh"

echo "FUCHSIA_ROOT: $FUCHSIA_ROOT"
if [ ! -d "$FUCHSIA_ROOT/.jiri_root" ]; then
    fuchsia_die "not a jiri Fuchsia checkout (no .jiri_root).
  Bootstrap one with:
    curl -s 'https://fuchsia.googlesource.com/fuchsia/+/HEAD/scripts/bootstrap?format=TEXT' | base64 -d | bash
  or set FUCHSIA_ROOT to an existing checkout."
fi

# Report the pinned revision.
if git -C "$FUCHSIA_ROOT" rev-parse --git-dir >/dev/null 2>&1; then
    echo "revision:    $(git -C "$FUCHSIA_ROOT" log -1 --format='%h %cd %s' --date=short)"
fi

echo "FUCHSIA_OUT: $FUCHSIA_OUT"
[ -f "$FUCHSIA_OUT/images.json" ] ||
    fuchsia_die "no build output in FUCHSIA_OUT (missing images.json).
  Build it: cd \$FUCHSIA_ROOT && fx set bringup.x64 && fx build
  or set FUCHSIA_OUT to an existing out/ directory."

fuchsia_resolve_images
echo "ZBI:         $FUCHSIA_ZBI ($(du -h "$FUCHSIA_ZBI" | cut -f1))"
if [ -n "$FUCHSIA_QEMU_KERNEL" ]; then
    echo "boot shim:   $FUCHSIA_QEMU_KERNEL"
else
    echo "boot shim:   (none — direct kernel boot only)"
fi

fuchsia_resolve_qemu
echo "QEMU:        $FUCHSIA_QEMU_BIN"
echo "KVM:         $([ "$FUCHSIA_KVM" = 1 ] && echo available || echo 'NOT available (TCG fallback, slow)')"

echo
echo "fuchsia-check: OK"
