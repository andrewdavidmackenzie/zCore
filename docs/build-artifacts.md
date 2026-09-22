# Build Artifacts and Generated Files

This document describes all build outputs, generated files, and cached
artifacts in the zCore project.

For project architecture, see
[architecture.md](architecture.md).

## Build Artifacts and Generated Files

### Cargo Build Output: `target/`

The standard Cargo output directory. Gitignored.

| Path                              | Generator     |
|-----------------------------------|---------------|
| `target/{arch}/release/zcore`     | `cargo build` |
| Kernel ELF for bare-metal.        | via xtask     |
| `{arch}` is the custom target     |               |
| triple (e.g., `aarch64`,          |               |
| `riscv64`).                       |               |
| `target/{arch}/release/zcore.bin` | objcopy via   |
| Stripped raw binary from ELF.     | `cargo bin`   |
| Used for riscv64 QEMU and some    |               |
| board targets.                    |               |
| `target/{arch}/release/build/`    | `cargo build` |
| Build script outputs (OUT_DIR).   |               |
| `target/release/`                 | `cargo build` |
| LibOS mode build output (host     | (libos)       |
| architecture).                    |               |
| `target/zcore.asm`                | `cargo asm`   |
| Kernel disassembly dump via       |               |
| `objdump -d`.                     |               |

### Filesystem Images: `target/qemu-{arch}/release/*.img`

SFS (Simple File System) images for QEMU, built by `cargo image`.

| Path | Generator |
|------|-----------|
| `target/qemu-aarch64/release/aarch64-linux.img` | `cargo image --arch aarch64` |
| `target/qemu-riscv64/release/riscv64-linux.img` | `cargo image --arch riscv64` |
| `target/qemu-aarch64/release/aarch64-zircon.img` | `cargo qemu --zircon` |
| `target/qemu-x86_64/release/x86_64-zircon.img` | `cargo qemu --zircon` |

### Rootfs Directories: `target/rootfs/`

Built by `cargo rootfs`. Removed by `cargo clean`.

| Path | Contents |
|------|----------|
| `target/rootfs/linux/aarch64/bin/busybox` | Statically linked busybox |
| `target/rootfs/linux/aarch64/bin/{sh,ls,cat,...}` | Symlinks to busybox |
| `target/rootfs/linux/aarch64/lib/ld-musl-aarch64.so.1` | Musl C library |
| `target/rootfs/linux/riscv64/` | Same layout for riscv64 |
| `target/rootfs/linux-libos/aarch64/` | LibOS rootfs (static-PIE busybox for macOS) |
| `target/rootfs/zircon/aarch64/bin/` | Petal ELF binaries |

### Build Script Generated Files

| Path                           | Generator     |
|--------------------------------|---------------|
| `zCore/src/platform/riscv/`    | `zCore/`      |
| `kernel-vars.ld`               | `build.rs`    |
| Generated linker script        |               |
| fragment with BASE_ADDRESS     |               |
| for riscv64. Gitignored.       |               |
| `$OUT_DIR/consts.rs`           | `linux-`      |
| (in target build dir)          | `syscall/`    |
| SyscallType enum from          | `build.rs`    |
| architecture .h.in files.      |               |
| `zircon-syscall/src/consts.rs` | `zircon-`     |
| Zircon SyscallType enum        | `syscall/`    |
| from zx-syscall-numbers.h.     | `build.rs`    |
| Written into source tree.      |               |
| `$OUT_DIR/shadow.rs`           | `xtask/`      |
| (in target build dir)          | `build.rs`    |
| Build metadata for `dump`.     | (shadow-rs)   |

`shadow-rs` is a build-time code generator that embeds VCS/build metadata as
Rust constants. `cargo xtask dump` prints: host OS, Rust channel, rustc/cargo
versions, build timestamp, git branch, short commit hash, author, and commit
date. Used for quickly checking what version is built and with what toolchain.
The generated `shadow.rs` contains `const` strings for each field.


### Build Cache: `.build-cache/`

Gitignored. Auto-populated by the build system. Total size ~600 MB when
fully populated. Survives `cargo clean` (which only removes `target/`).

**Downloaded origins (`.build-cache/origin/`):**

| Path | Contents |
|------|----------|
| `origin/archs/aarch64/Aarch64_firmware.zip` | UEFI firmware archive |
| `origin/archs/riscv64/riscv64-linux-musl-cross.tgz` | musl cross-compiler (~103 MB) |
| `origin/archs/x86_64/prebuilt.tar.xz` | Zircon prebuilts |
| `origin/repos/busybox/` | Cloned busybox source repo |
| `origin/repos/ffmpeg/` | Cloned FFmpeg (optional) |
| `origin/repos/opencv/` | Cloned OpenCV (optional) |

- `Aarch64_firmware.zip`: UEFI firmware for QEMU aarch64 (QEMU_EFI.fd +
  bootloader EFI app). Downloaded by xtask during first aarch64 build.

  
Note: the UEFI firmware is a legacy artifact from the UEFI boot path. The
current xtask build uses QEMU's `-kernel` flag (direct kernel load) and does
not require UEFI firmware for aarch64.

- `riscv64-linux-musl-cross.tgz`: Complete GCC 11.2.1 cross-compiler toolchain
  for riscv64- linux-musl. ~103 MB compressed, ~357 MB extracted. Linux-host
  only (macOS uses Homebrew).
- `prebuilt.tar.xz`: Zircon prebuilt binaries (userboot.so, libzircon.so,
  bringup.zbi) for x86_64. Legacy x86_64 path only (not active). Superseded by userstart (#121) for aarch64
  since Zircon mode and x86_64 are not active. Would be used if x86_64 is
  resurrected (see
  [#94](https://github.com/andrewdavidmackenzie/zCore/issues/94)).
- `rootfs/libos/`: LibOS rootfs, built locally from busybox source using
  musl cross-compiler for the host architecture. Created by `cargo xtask
  libos-libc-test` or `make libos-build`.
  > - `busybox/`: Git clone of official busybox repo,
  >   used as source for cross-compilation.
  > - `ffmpeg/`, `opencv/`: Optional media library
  >   sources for cross-compilation demos.

**Built/extracted outputs (`.build-cache/target/`):**

Compiled toolchains and busybox. Kept under `.build-cache/` so they
survive `cargo clean`.

| Path | Contents |
|------|----------|
| `.build-cache/target/aarch64/busybox/` | Compiled busybox for aarch64 (~53 MB) |
| `.build-cache/target/aarch64/firmware/` | Extracted QEMU_EFI.fd, bootloader |
| `.build-cache/target/riscv64/busybox/` | Compiled busybox for riscv64 |
| `.build-cache/target/riscv64/riscv64-linux-musl-cross/` | Extracted GCC cross-compiler (~357 MB) |
| `.build-cache/target/{arch}/ffmpeg/` | FFmpeg build (optional) |
| `.build-cache/target/{arch}/opencv/` | OpenCV build (optional) |

### Other Generated Artifacts

| Path                        | Generator     |
|-----------------------------|---------------|
| `zCore/disk/`               | Build system  |
| EFI boot disk for aarch64   | (aarch64 UEFI |
| UEFI boot. Contains         | path). Git-   |
| bootaa64.efi and Boot.json. | ignored.      |
| `zCore/zcore.bin.gz`        | Makefile      |
| Gzipped kernel for SiFive   | (fu740 build) |
| FU740 board.                |               |
| `zCore/zcore-fu740.itb`     | mkimage       |
| FIT image for FU740 U-Boot. | (fu740 build) |
| `zCore/uImageC910`          | mkimage       |
| uImage for T-HEAD C910.     | (c910 build)  |

### What `make clean` Removes

- **`make clean`**: `cargo clean` (removes `target/`, which includes rootfs,
  images, and kernel binaries), plus legacy locations
- **`make cleanup`**: Above + `rm -rf .build-cache/target`
  (removes extracted toolchains and busybox builds)
- **`make clean-everything`**: Above +
  `rm -rf .build-cache` (removes ALL downloads, cloned repos, and build caches)

---

## Usage Status Summary
