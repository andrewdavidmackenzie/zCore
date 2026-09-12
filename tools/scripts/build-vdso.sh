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
    AS="aarch64-none-elf-as"
    LD="aarch64-none-elf-ld"
    OBJCOPY="aarch64-none-elf-objcopy"
    ;;
  x86_64)
    RUST_TARGET="x86_64-unknown-none"
    AS="x86_64-elf-as"
    LD="x86_64-elf-ld"
    OBJCOPY="x86_64-elf-objcopy"
    ;;
  riscv64)
    RUST_TARGET="riscv64gc-unknown-none-elf"
    AS="riscv64-unknown-elf-as"
    LD="riscv64-unknown-elf-ld"
    OBJCOPY="riscv64-unknown-elf-objcopy"
    ;;
  *)
    echo "ERROR: unsupported architecture '$ARCH'"
    exit 1
    ;;
esac

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

# Step 4: Link with linker script
"$LD" -T zCore/vdso/vdso.ld "$VDSO_DIR/vdso.o" -o "$VDSO_DIR/vdso.elf" 2>&1 || true
echo "   ELF: $VDSO_DIR/vdso.elf"

# Step 5: Extract flat binary
"$OBJCOPY" -O binary "$VDSO_DIR/vdso.elf" "$VDSO_DIR/vdso.bin"

SIZE=$(wc -c < "$VDSO_DIR/vdso.bin")
SYMS=$("$OBJCOPY" --dump-section .text=/dev/stdout "$VDSO_DIR/vdso.elf" 2>/dev/null | wc -c || echo "?")
echo "   Binary: $VDSO_DIR/vdso.bin ($SIZE bytes)"
echo "==> vDSO build complete for $ARCH"
