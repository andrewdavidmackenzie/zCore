#!/usr/bin/env bash
#
# Build the vDSO flat binary for a given architecture.
#
# Usage: tools/scripts/build-vdso.sh <arch>
#
# This script:
#   1. Builds the zcore-vdso crate to generate assembly
#   2. Assembles the trampolines with the system assembler
#   3. Links with the vDSO linker script
#   4. Produces a flat binary at target/vdso/<arch>/vdso.bin
#
# The binary contains only the syscall trampolines (code).
# VdsoConstants data is written by the kernel at boot, not baked in.

set -euo pipefail

ARCH="${1:?Usage: $0 <arch>}"

case "$ARCH" in
  aarch64)
    RUST_TARGET="aarch64-unknown-none-softfloat"
    CROSS="aarch64-none-elf"
    ELF_FORMAT="elf64-littleaarch64"
    ;;
  x86_64)
    RUST_TARGET="x86_64-unknown-none"
    CROSS="x86_64-elf"
    ELF_FORMAT="elf64-x86-64"
    ;;
  riscv64)
    RUST_TARGET="riscv64gc-unknown-none-elf"
    CROSS="riscv64-unknown-elf"
    ELF_FORMAT="elf64-littleriscv"
    ;;
  *)
    echo "ERROR: unsupported architecture '$ARCH'"
    exit 1
    ;;
esac

AS="${CROSS}-as"
LD="${CROSS}-ld"
OBJCOPY="${CROSS}-objcopy"
NM="${CROSS}-nm"

echo "==> Building vDSO for $ARCH..."

# Step 1: Build the crate to generate assembly
cargo build -p zcore-vdso --target "$RUST_TARGET" --release 2>&1 | tail -2

# Step 2: Find the generated assembly file
OUT_DIR=$(find "target/$RUST_TARGET/release/build" -name "vdso_trampolines.S" 2>/dev/null | head -1)
if [ -z "$OUT_DIR" ]; then
  echo "ERROR: could not find generated vdso_trampolines.S"
  exit 1
fi
echo "   Assembly: $OUT_DIR"

# Step 3: Assemble
VDSO_DIR="target/vdso/$ARCH"
mkdir -p "$VDSO_DIR"
"$AS" "$OUT_DIR" -o "$VDSO_DIR/vdso.o"
echo "   Object: $VDSO_DIR/vdso.o"

# Step 4: Link as shared library with linker script
# Use -shared to produce a proper ELF .so with dynamic symbols.
# Use --soname to set the library name that Fuchsia binaries expect.
# Use --export-dynamic to export all zx_* symbols.
# Patch OUTPUT_FORMAT in linker script for the target architecture.
LDSCRIPT="$VDSO_DIR/vdso.ld"
sed "s/elf64-littleaarch64/$ELF_FORMAT/" zCore/vdso/vdso.ld > "$LDSCRIPT"

"$LD" -shared --soname=libzircon.so --export-dynamic \
  -T "$LDSCRIPT" "$VDSO_DIR/vdso.o" \
  -o "$VDSO_DIR/libzircon.so" 2>&1
echo "   Shared library: $VDSO_DIR/libzircon.so"

# Also produce a flat binary for kernel embedding
"$OBJCOPY" -O binary "$VDSO_DIR/libzircon.so" "$VDSO_DIR/vdso.bin"

# Step 6: Verify the ELF structure
READELF="${CROSS}-readelf"
ERRORS=0

echo "==> Verifying vDSO ELF..."

# Check ELF type is DYN (shared object)
ELF_TYPE=$("$READELF" -h "$VDSO_DIR/libzircon.so" 2>/dev/null | grep "Type:" | awk '{print $2}')
if [ "$ELF_TYPE" = "DYN" ]; then
  echo "   [OK] ELF type: DYN (shared object)"
else
  echo "   [FAIL] ELF type: expected DYN, got '$ELF_TYPE'"
  ERRORS=$((ERRORS + 1))
fi

# Check soname is libzircon.so
SONAME=$("$READELF" -d "$VDSO_DIR/libzircon.so" 2>/dev/null | grep SONAME | sed 's/.*\[//' | sed 's/\]//')
if [ "$SONAME" = "libzircon.so" ]; then
  echo "   [OK] SONAME: libzircon.so"
else
  echo "   [FAIL] SONAME: expected 'libzircon.so', got '$SONAME'"
  ERRORS=$((ERRORS + 1))
fi

# Count dynamic text symbols
SYM_COUNT=$("$NM" -D "$VDSO_DIR/libzircon.so" 2>/dev/null | grep -c " T " || echo "0")
if [ "$SYM_COUNT" -ge 150 ]; then
  echo "   [OK] Dynamic symbols: $SYM_COUNT (>= 150 expected)"
else
  echo "   [FAIL] Dynamic symbols: $SYM_COUNT (expected >= 150)"
  ERRORS=$((ERRORS + 1))
fi

# Check critical symbols are present
for sym in zx_channel_create zx_handle_close zx_process_exit zx_vmo_create \
           zx_object_wait_one zx_thread_exit zx_futex_wait zx_nanosleep; do
  if "$NM" -D "$VDSO_DIR/libzircon.so" 2>/dev/null | grep -q " T $sym\$"; then
    : # ok
  else
    echo "   [FAIL] Missing critical symbol: $sym"
    ERRORS=$((ERRORS + 1))
  fi
done
if [ "$ERRORS" -eq 0 ]; then
  echo "   [OK] All critical symbols present"
fi

# Check VdsoConstants data page exists at offset 0x7000
VDSO_SECTION=$("$READELF" -S "$VDSO_DIR/libzircon.so" 2>/dev/null | grep "vdso_constants")
if [ -n "$VDSO_SECTION" ]; then
  echo "   [OK] .vdso_constants section present"
else
  echo "   [FAIL] .vdso_constants section not found"
  ERRORS=$((ERRORS + 1))
fi

# Report sizes
SO_SIZE=$(wc -c < "$VDSO_DIR/libzircon.so" | tr -d ' ')
BIN_SIZE=$(wc -c < "$VDSO_DIR/vdso.bin" | tr -d ' ')
echo ""
echo "   Shared lib: $SO_SIZE bytes"
echo "   Flat binary: $BIN_SIZE bytes (for kernel embedding)"

if [ "$ERRORS" -gt 0 ]; then
  echo ""
  echo "==> FAIL: $ERRORS verification error(s)"
  exit 1
fi

echo "==> vDSO build and verification complete for $ARCH"
