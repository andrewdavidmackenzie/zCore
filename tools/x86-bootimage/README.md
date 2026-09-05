# x86-bootimage

Helper tool that creates bootable x86_64 disk images for zCore.

## What it does

Takes the zCore kernel ELF binary and (optionally) a rootfs SFS image,
and produces a single bootable disk image that can be loaded by QEMU or
written to a USB drive. The tool uses the
[`bootloader`](https://crates.io/crates/bootloader) crate (v0.11) to
wrap the kernel in a BIOS-compatible boot sector and FAT filesystem.

The resulting image contains:
- A BIOS bootloader (SeaBIOS -> bootloader stages 1-4)
- The zCore kernel ELF
- An optional ramdisk (rootfs SFS image), exposed to the kernel via
  `BootInfo.ramdisk_addr` / `BootInfo.ramdisk_len`

## Why it exists

Unlike aarch64 (where QEMU loads the kernel ELF directly via `-kernel`),
x86_64 requires a proper boot image. The `bootloader` crate handles the
complex x86 boot sequence (real mode -> protected mode -> long mode,
page table setup, GDT, etc.) and passes control to the kernel with a
well-defined `BootInfo` structure.

This tool lives in `tools/` rather than as a build dependency because it
is a **host tool** (runs on the build machine) that depends on the
`bootloader` crate, which internally compiles separate stage binaries
for the x86_64 target. Keeping it separate avoids polluting the kernel's
dependency tree.

## Usage

```bash
x86-bootimage <kernel-elf> <output-image> [--ramdisk <rootfs-image>]
```

Examples:
```bash
# Kernel only (no rootfs -- kernel will panic at rootfs mount)
x86-bootimage target/x86_64/release/zcore boot.img

# Kernel + rootfs (normal boot to shell)
x86-bootimage target/x86_64/release/zcore boot.img --ramdisk zCore/x86_64.img
```

The tool is built and invoked automatically by the xtask build system
(`cargo qemu --arch x86_64`) and the boot-test script
(`tools/scripts/boot-test.sh x86_64`).

## UEFI support

Currently BIOS-only. UEFI boot is blocked by an upstream issue in the
`bootloader` crate ([rust-osdev/bootloader#579](https://github.com/rust-osdev/bootloader/issues/579)).
Tracked in [#151](https://github.com/andrewdavidmackenzie/zCore/issues/151).
When the fix lands, enable the `uefi` feature in `Cargo.toml` and add
UEFI support to this tool.
