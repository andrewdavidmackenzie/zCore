#!/usr/bin/env bash
#
# Write a zCore boot image to an SD card, dispatching on the board
# (the board's flash-method = "sd" routes here).
#
# Usage: tools/flash-sd.sh <sd-mount-point> <product.board>
#   SD mount point: the mounted FAT32 boot partition of the SD card.
#
# Boards:
#   raspi400  — copies kernel + config to the Pi boot partition
#   jolla-c2  — writes an ext4 boot image (needs DISK=, see
#               tools/jolla-c2/make-sdcard-image.sh)

set -euo pipefail

SD="${1:?Usage: $0 <sd-mount-point> <product.board>}"
CONFIG="${2:?Usage: $0 <sd-mount-point> <product.board>}"
BOARD="${CONFIG##*.}"

case "$BOARD" in
  raspi400 | raspi400-uefi)
    exec "$(dirname "$0")/raspi/prepare-sd.sh" "$SD"
    ;;
  jolla-c2)
    if [ -z "${DISK:-}" ]; then
      echo "ERROR: jolla-c2 needs DISK=<raw-disk-device> (not a mount point)" >&2
      echo "Usage: make flash CONFIG=petal.jolla-c2 DISK=/dev/diskN" >&2
      exit 1
    fi
    exec "$(dirname "$0")/jolla-c2/make-sdcard-image.sh" "$DISK"
    ;;
  *)
    echo "ERROR: board '$BOARD' has no SD flash procedure" >&2
    exit 1
    ;;
esac
