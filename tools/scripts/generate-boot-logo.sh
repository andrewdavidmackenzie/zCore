#!/usr/bin/env bash
#
# Regenerate the UEFI boot logo BMP from the SVG source.
#
# The output BMP must be an 8-bit paletted, uncompressed Windows V3 BMP
# that fits within ~185 KB (the size budget of the EDK2 LogoDxe slot in
# the pftf RPi4 firmware).
#
# Prerequisites:
#   - rsvg-convert  (librsvg, e.g. `brew install librsvg`)
#   - magick        (ImageMagick 7, e.g. `brew install imagemagick`)
#
# Usage:
#   tools/scripts/generate-boot-logo.sh
#
# Outputs:
#   assets/images/zirconia-boot-logo.bmp  (crystal-only, 640x287, ~180 KB)

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/../.." && pwd)"

SVG_SRC="$PROJECT_DIR/assets/images/zirconia-boot-logo.svg"
BMP_OUT="$PROJECT_DIR/assets/images/zirconia-boot-logo.bmp"

# Maximum BMP file size (bytes).  The original RPi Logo.bmp is 185012 bytes.
MAX_BMP_SIZE=185012

# ── Prerequisites ──────────────────────────────────────────────────────
for cmd in rsvg-convert magick; do
    if ! command -v "$cmd" >/dev/null 2>&1; then
        echo "ERROR: $cmd not found. Install librsvg and ImageMagick."
        exit 1
    fi
done

if [ ! -f "$SVG_SRC" ]; then
    echo "ERROR: SVG source not found: $SVG_SRC"
    exit 1
fi

# ── Render ─────────────────────────────────────────────────────────────
TMPDIR="$(mktemp -d)"
trap 'rm -rf "$TMPDIR"' EXIT

echo "==> Rendering SVG -> PNG (640px wide)..."
rsvg-convert -w 640 "$SVG_SRC" -o "$TMPDIR/logo.png"

echo "==> Converting PNG -> 8-bit paletted BMP..."
magick "$TMPDIR/logo.png" \
    -colors 255 -compress None -type Palette \
    BMP3:"$BMP_OUT"

# ── Verify size ────────────────────────────────────────────────────────
BMP_SIZE=$(wc -c < "$BMP_OUT" | tr -d ' ')
if [ "$BMP_SIZE" -gt "$MAX_BMP_SIZE" ]; then
    echo "ERROR: BMP is $BMP_SIZE bytes (max $MAX_BMP_SIZE). Reduce render width."
    exit 1
fi

echo "==> Generated: $BMP_OUT ($BMP_SIZE bytes)"
magick identify "$BMP_OUT"
