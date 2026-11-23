#!/usr/bin/env bash
#
# Import Fuchsia userspace artifacts into zCore's prebuilt/ tree
# (issue #523 Phase B). Modelled on prebuilt/zircon-test: zCore builds
# must be reproducible without a Fuchsia checkout, so the artifacts are
# snapshotted into the repo rather than referenced live.
#
# Extracts from the Fuchsia build's fuchsia.zbi (resolved via
# images.json — see fuchsia-env.sh):
#
#   userboot.elf           the real Fuchsia first process (static PIE),
#                          carved from the kernel package (ZBI STORAGE_
#                          KERNEL item -> zstd -> bootfs "zircon/userboot")
#   userboot-test-rust.elf minimal test userboot ("zircon/userboot-test-
#                          rust") for validating the bootstrap protocol
#   vdso.elf               the vDSO ("zircon/vdso") — matched to the
#                          userboot from the same kernel image
#   fuchsia.zbi            the complete boot image (kernel item ignored
#                          by zCore; bootfs + cmdline items are used)
#   PROVENANCE.md          source revision, file list, sha256s
#
# The extraction parses the ZBI and bootfs formats directly (stable
# ABIs) rather than Fuchsia build-internal staging paths, so it works
# from any Fuchsia build output. Decompression uses the Python >= 3.14
# stdlib zstd module, falling back to the zstd(1) CLI.
#
# Everything is staged in a temp directory and moved into place only
# after every extraction and copy succeeds, so a failure mid-import
# cannot leave a half-updated prebuilt/fuchsia/.
#
# Usage: tools/scripts/fuchsia-import.sh [arch]
#   arch: x86_64 (default) or aarch64 — the prebuilt/fuchsia/<arch>/
#         subdir to populate. The arch is inferred from FUCHSIA_OUT's
#         name when not given.
#
# Exit code 0 = artifacts imported (or already up to date)
# Exit code 1 = missing tree/build/artifacts or parse failure

set -euo pipefail

source "$(dirname "$0")/fuchsia-env.sh"

# Only the ZBI is needed here — the QEMU boot shim is a run/test
# concern, and a build without one (e.g. a product bundle) can still
# be a valid import source.
fuchsia_resolve_zbi

# Infer the target arch from the build dir name (bringup.x64-balanced,
# bringup.qemu-arm64-balanced, ...), overridable as $1.
ARCH="${1:-}"
if [ -z "$ARCH" ]; then
    case "$(basename "$FUCHSIA_OUT")" in
        *x64*) ARCH=x86_64 ;;
        *arm64*) ARCH=aarch64 ;;
        *) fuchsia_die "cannot infer arch from FUCHSIA_OUT='$FUCHSIA_OUT'; pass it: $0 <x86_64|aarch64>" ;;
    esac
fi

DEST="$ZCORE_ROOT/prebuilt/fuchsia/$ARCH"
mkdir -p "$DEST"

echo "fuchsia-import: FUCHSIA_OUT=$FUCHSIA_OUT"
echo "fuchsia-import: ZBI=$FUCHSIA_ZBI"
echo "fuchsia-import: dest=$DEST"

REV=""
if git -C "$FUCHSIA_ROOT" rev-parse --git-dir >/dev/null 2>&1; then
    REV="$(git -C "$FUCHSIA_ROOT" log -1 --format='%H (%cd %s)' --date=short)"
fi

# Stage everything here; move into DEST only once all of it exists.
STAGE="$(mktemp -d)"
trap 'rm -rf "$STAGE"' EXIT

# Carve the kernel-package files out of the ZBI. Python does the
# format parsing; the shell handles staging and provenance.
python3 - "$FUCHSIA_ZBI" "$STAGE" <<'EOF'
import hashlib
import struct
import sys
from pathlib import Path

zbi_path, stage_dir = sys.argv[1], Path(sys.argv[2])

ITEM_MAGIC = 0xB5781729
TYPE_CONTAINER = 0x544F4F42  # "BOOT"
TYPE_STORAGE_KERNEL = 0x5254534B  # "KSTR"
BOOTFS_MAGIC = 0xA56D3FF9
ZSTD_MAGIC = b"\x28\xb5\x2f\xfd"

# Files to carve from the kernel package bootfs -> output name.
WANTED = {
    "zircon/userboot": "userboot.elf",
    "zircon/userboot-test-rust": "userboot-test-rust.elf",
    "zircon/vdso": "vdso.elf",
}


def zstd_decompress(data: bytes) -> bytes:
    try:
        from compression import zstd  # Python >= 3.14

        return zstd.decompress(data)
    except ImportError:
        pass
    import subprocess

    return subprocess.run(
        ["zstd", "-d", "--stdout", "-"],
        input=data,
        stdout=subprocess.PIPE,
        check=True,
    ).stdout


def find_kernel_payload(zbi: bytes) -> bytes:
    """Return the STORAGE_KERNEL ("KSTR") item payload of a ZBI file."""
    ctype, clen = struct.unpack_from("<II", zbi, 0)
    if ctype != TYPE_CONTAINER:
        sys.exit("not a ZBI container")
    off, end = 32, min(32 + clen, len(zbi))
    while off + 32 <= end:
        itype, ilen = struct.unpack_from("<II", zbi, off)
        (magic,) = struct.unpack_from("<I", zbi, off + 24)
        if magic != ITEM_MAGIC:
            sys.exit(f"bad item magic at {off:#x}")
        if itype == TYPE_STORAGE_KERNEL:
            return zbi[off + 32 : off + 32 + ilen]
        off += 32 + ((ilen + 7) & ~7)
    sys.exit("no STORAGE_KERNEL item in ZBI (not a full boot image?)")


def parse_bootfs(img: bytes) -> dict:
    """Parse a bootfs image into {name: (data_off, data_len)}."""
    magic, dirsize = struct.unpack_from("<II", img, 0)
    if magic != BOOTFS_MAGIC:
        sys.exit(f"bad bootfs magic {magic:#x}")
    entries = {}
    off = 16  # zbi_bootfs_header_t: magic, dirsize, reserved0, reserved1
    end = 16 + dirsize
    while off + 12 <= end:
        name_len, data_len, data_off = struct.unpack_from("<III", img, off)
        if name_len == 0:
            break
        name = img[off + 12 : off + 12 + name_len].rstrip(b"\0").decode()
        # Bounds check: Python slicing would silently shorten an
        # out-of-range range, producing a truncated artifact.
        if data_off > len(img) or data_len > len(img) - data_off:
            sys.exit(
                f"bootfs entry '{name}' range [{data_off:#x}, {data_off + data_len:#x}) "
                f"exceeds the kernel package size {len(img):#x} — corrupt image?"
            )
        entries[name] = (data_off, data_len)
        off += (12 + name_len + 3) & ~3
    return entries


zbi = Path(zbi_path).read_bytes()
payload = find_kernel_payload(zbi)
if payload[:4] != ZSTD_MAGIC:
    sys.exit("STORAGE_KERNEL payload is not zstd-compressed")
kernel_pkg = zstd_decompress(payload)
print(f"kernel package: {len(payload)} -> {len(kernel_pkg)} bytes")

entries = parse_bootfs(kernel_pkg)
print(f"bootfs: {len(entries)} files")

for src, dest_name in WANTED.items():
    if src not in entries:
        sys.exit(f"'{src}' not found in kernel package bootfs")
    data_off, data_len = entries[src]
    data = kernel_pkg[data_off : data_off + data_len]
    if data[:4] != b"\x7fELF":
        sys.exit(f"'{src}' is not an ELF")
    out = stage_dir / dest_name
    out.write_bytes(data)
    print(f"  {src} -> {dest_name} ({data_len} bytes, sha256 {hashlib.sha256(data).hexdigest()[:12]}…)")
EOF

# Stage the full ZBI too (bootfs + cmdline items; the kernel item is
# unused by zCore but kept so the artifact is the complete image).
cp -f "$FUCHSIA_ZBI" "$STAGE/fuchsia.zbi"

# Provenance: what was imported, from where, with which hashes.
sha256_of() {
    if command -v sha256sum >/dev/null 2>&1; then
        sha256sum "$1" | cut -d' ' -f1
    else
        shasum -a 256 "$1" | cut -d' ' -f1 # macOS
    fi
}
{
    echo "# Fuchsia userspace artifacts ($ARCH)"
    echo
    echo "Imported by \`make fuchsia-import\` (tools/scripts/fuchsia-import.sh)"
    echo "from a Fuchsia build output. Do not edit by hand."
    echo
    echo "- Fuchsia revision: ${REV:-unknown}"
    echo "- Build directory: \`$FUCHSIA_OUT\`"
    echo "- Boot image: \`fuchsia.zbi\` (kernel item unused by zCore; userboot"
    echo "  and the vDSO are carved from its kernel package)"
    echo
    echo "| File | Source (kernel package bootfs) | Size | sha256 |"
    echo "|------|-------------------------------|------|--------|"
    for f in userboot.elf userboot-test-rust.elf vdso.elf fuchsia.zbi; do
        [ -f "$STAGE/$f" ] || continue
        src="copied from images.json zircon-a"
        case "$f" in
            userboot.elf) src="zircon/userboot" ;;
            userboot-test-rust.elf) src="zircon/userboot-test-rust" ;;
            vdso.elf) src="zircon/vdso" ;;
        esac
        printf '| `%s` | `%s` | %s | `%s` |\n' \
            "$f" "$src" "$(wc -c <"$STAGE/$f" | tr -d ' ')" "$(sha256_of "$STAGE/$f")"
    done
} > "$STAGE/PROVENANCE.md"

# Everything succeeded — move the staged bundle into place.
for f in userboot.elf userboot-test-rust.elf vdso.elf fuchsia.zbi PROVENANCE.md; do
    [ -f "$STAGE/$f" ] || fuchsia_die "internal error: $f missing from staging"
    mv -f "$STAGE/$f" "$DEST/$f"
done

echo "fuchsia-import: DONE"
