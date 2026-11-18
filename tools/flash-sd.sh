#!/usr/bin/env bash
#
# Write a zCore boot image to an SD card, dispatching on the board
# (the board's flash-method = "sd" routes here).
#
# Usage: tools/flash-sd.sh <sd-mount-point> <product.board>
#   raspi400: the SD mount point is required (FAT32 boot partition).
#   jolla-c2: SD is unused; DISK=<raw-disk-device> is required instead
#             (see tools/jolla-c2/make-sdcard-image.sh).

set -euo pipefail

SD="${1:-}"
CONFIG="${2:?Usage: $0 <sd-mount-point> <product.board>}"
BOARD="${CONFIG##*.}"

case "$BOARD" in
  raspi400)
    if [ -z "$SD" ]; then
      echo "ERROR: raspi400 needs SD=<mount-point-of-FAT32-partition>" >&2
      echo "Usage: make flash CONFIG=<product>.raspi400 SD=/Volumes/boot" >&2
      exit 1
    fi
    exec "$(dirname "$0")/raspi/prepare-sd-uefi.sh" "$SD" "$CONFIG"
    ;;
  jolla-c2)
    if [ -z "${DISK:-}" ]; then
      echo "ERROR: jolla-c2 needs DISK=<raw-disk-device> (not a mount point)" >&2
      echo "Usage: make flash CONFIG=petal.jolla-c2 DISK=/dev/diskN" >&2
      exit 1
    fi
    exec "$(dirname "$0")/jolla-c2/make-sdcard-image.sh" "$DISK" "$CONFIG"
    ;;
  *)
    echo "ERROR: board '$BOARD' has no SD flash procedure" >&2
    exit 1
    ;;
esac
