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

### Filesystem Images: `zCore/*.img`

Yes, images could go to `target/{arch}/release/ zcore.img`. The xtask QEMU
launcher would need to look there instead of `zCore/`. Straightforward
change. See [#79](https://github.com/andrewdavidmackenzie/zCore/issues/79).


| Path                  | Generator          |
|-----------------------|--------------------|
| `zCore/aarch64.img`   | `cargo image`      |
|   SFS image from      | (xtask             |
|   `rootfs/aarch64/`.  | LinuxRootfs::      |
|   Used as virtio-blk  | image())           |
|   drive in QEMU.      |                    |
| `zCore/riscv64.img`   | `cargo image`      |
|   SFS image from      |                    |
|   `rootfs/riscv64/`.  |                    |
|   Used as initrd.     |                    |

NOTE: Explain SFS briefly or add a link to read more about it.

### Rootfs Directories: `rootfs/{arch}/`

Yes, `rootfs/` could become `target/{arch}/rootfs/` since it's a build
intermediate. The xtask image builder and QEMU launcher would need path
updates. Would make `make clean` simpler (just `cargo clean`). Covered by the
artifact consolidation issue above.


Gitignored. Built by `cargo rootfs`.

| Path                          | Contents      |
|-------------------------------|---------------|
| `rootfs/aarch64/bin/busybox`  | Statically    |
|                               | linked        |
|                               | busybox       |
| `rootfs/aarch64/bin/{sh,ls,`  | Symlinks to   |
| `cat,...}`                    | busybox       |
| `rootfs/aarch64/lib/ld-musl-` | Musl C        |
| `aarch64.so.1`                | library       |
| `rootfs/aarch64/bin/libc-`    | Compiled      |
| `test/`                       | libc-test     |
|                               | executables   |
| `rootfs/riscv64/`             | Same layout   |
|                               | for riscv64   |
| `rootfs/libos/`               | LibOS rootfs  |
|                               | (downloaded)  |

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


### xtask Cache: `ignored/`

Better names: `.build-cache/`, `build-deps/`, or `extern/`. The `ignored/` name
only makes sense in the context of `.gitignore`. Something like `.build-cache/`
(with the dot for hidden) or `build-deps/` (descriptive) would be clearer.
Low priority rename. Could be part of [#79](https://github.com/andrewdavidmackenzie/zCore/issues/79).


Gitignored. Auto-populated by the build system. Total size ~600 MB when fully
populated.

**Downloaded origins (`ignored/origin/`):**

| Path                           | Contents      |
|--------------------------------|---------------|
| `origin/archs/aarch64/`        | UEFI firmware |
| `Aarch64_firmware.zip`         | archive       |
| `origin/archs/riscv64/`        | musl cross-   |
| `riscv64-linux-musl-cross.tgz` | compiler      |
|                                | (~103 MB)     |
| `origin/archs/x86_64/`         | Zircon        |
| `prebuilt.tar.xz`              | prebuilts     |
| `origin/archs/libos/`          | LibOS rootfs  |
| `rootfs-libos.tar.gz`          | archive       |
| `origin/repos/busybox/`        | Cloned        |
|                                | busybox       |
|                                | source repo   |
| `origin/repos/ffmpeg/`         | Cloned FFmpeg |
|                                | (optional)    |
| `origin/repos/opencv/`         | Cloned OpenCV |
|                                | (optional)    |

- `Aarch64_firmware.zip`: UEFI firmware for QEMU aarch64 (QEMU_EFI.fd +
  bootloader EFI app). Downloaded by xtask during first aarch64 build.

  
Note: the UEFI firmware is a legacy artifact from the UEFI boot path. The
current xtask build uses QEMU's `-kernel` flag (direct kernel load) and does
not require UEFI firmware for aarch64.

- `riscv64-linux-musl-cross.tgz`: Complete GCC 11.2.1 cross-compiler toolchain
  for riscv64- linux-musl. ~103 MB compressed, ~357 MB extracted. Linux-host
  only (macOS uses Homebrew).
- `prebuilt.tar.xz`: Zircon prebuilt binaries (userboot.so, libzircon.so,
  bringup.zbi) for x86_64. No longer used -- superseded by userstart (#121)
  since Zircon mode and x86_64 are not active. Would be used if x86_64 is
  resurrected (see
  [#94](https://github.com/andrewdavidmackenzie/zCore/issues/94)).
- `rootfs-libos.tar.gz`: Pre-built x86_64 musl rootfs for libos testing.
  Downloaded by `cargo libos-libc-test`.
  > - `busybox/`: Git clone of official busybox repo,
  >   used as source for cross-compilation.
  > - `ffmpeg/`, `opencv/`: Optional media library
  >   sources for cross-compilation demos.

**Built/extracted outputs (`ignored/target/`):**

Yes, `ignored/target/` contents could move under `target/build-deps/{arch}/` or
similar. The separation exists because `cargo clean` deletes `target/` but
`ignored/target/` survives (saving ~400 MB of toolchain re-download). If
merged, `cargo clean` would force re-download. A middle ground: keep downloaded
archives in `ignored/ origin/` but build outputs in `target/`. Covered by
artifact consolidation issue.


| Path                           | Contents      |
|--------------------------------|---------------|
| `target/aarch64/busybox/`      | Compiled      |
|                                | busybox for   |
|                                | aarch64       |
|                                | (statically   |
|                                | linked,       |
|                                | ~53 MB with   |
|                                | build tree)   |
| `target/aarch64/firmware/`     | Extracted     |
|                                | QEMU_EFI.fd,  |
|                                | bootloader,   |
|                                | Boot.json     |
| `target/riscv64/busybox/`      | Compiled      |
|                                | busybox for   |
|                                | riscv64       |
|                                | (dynamically  |
|                                | linked)       |
| `target/riscv64/riscv64-`      | Extracted     |
| `linux-musl-cross/`            | GCC 11.2.1    |
|                                | cross-        |
|                                | compiler      |
|                                | toolchain     |
|                                | (~357 MB)     |
| `target/{arch}/ffmpeg/`        | FFmpeg build  |
|                                | (optional)    |
| `target/{arch}/opencv/`        | OpenCV build  |
|                                | (optional)    |

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

- **`make clean`**: `cargo clean` (removes
`target/`), `rm -f *.asm`, `rm -rf rootfs`, `rm -rf zCore/disk`, all `*.img`
and `*.bin` under `zCore/`
- **`make cleanup`**: Above + `rm -rf ignored/target`
  (removes extracted toolchains and busybox builds)
- **`make clean-everything`**: Above +
`rm -rf ignored` (removes ALL downloads, cloned repos, and build caches)

---

## Usage Status Summary
