# Non-Crate Folders Reference

This document describes the non-Rust-crate directories in the zCore project.

For crate descriptions, see [architecture.md](architecture.md).
For build artifacts, see [build-artifacts.md](build-artifacts.md).

## Non-Crate Folders

### `.cargo/` -- Cargo Configuration

**Purpose:** Defines 17 cargo command aliases that route through xtask (e.g.,
`cargo qemu` -> `cargo run --package xtask --release -- qemu`).

**Status:** Actively used. Critical for the build workflow.

---

### `.github/` -- CI/CD Configuration

**Purpose:** GitHub Actions workflows and helper scripts.

**Workflows:**
- `build.yml` -- Format check + workspace build + bare-metal builds
  (aarch64/riscv64), runs on Ubuntu and macOS
- `test.yml` -- Unit tests + boot smoke test + libc conformance tests
  (aarch64), runs on Ubuntu and macOS

**Helper scripts:**
- `scripts/add-doc-index.sh` -- Documentation redirect page

It creates a `target/doc/index.html` that auto- redirects browsers to
`kernel_hal/index.html`. Used after `cargo doc` so GitHub Pages doc site lands
on kernel_hal docs instead of showing a directory listing. A simple HTML meta-
refresh.


- `scripts/make-qemu.sh` -- Build QEMU from source for CI

CI does NOT use this script. Workflows install QEMU via `apt install qemu-
system-arm qemu-system-misc` (Linux) or `brew install qemu` (macOS). This
script is legacy/unused. Could be removed or moved to `tools/` for reference.


**Status:** Actively used. Runs on every push and PR.

---

### `config/` -- Machine Configuration Files

**Purpose:** Contains `machine-features.toml` which defines all supported
machine targets with their architecture, PCI support, and feature flags.

**Status:** Actively used. Read at build time by `z-config`.

It's already only 31 lines of TOML defining 7 machines. The two-level
`[manufacturer.product]` nesting is informational only -- z-config matches on
product name, ignoring manufacturer. Could flatten to `[machines.virt-aarch64]`
with optional `manufacturer` field, but gains are marginal. The real
simplification would be to inline z-config into xtask (see z-config TODO).


---

### `docs/` -- Documentation

**Purpose:** Project documentation including English README,

The main `README.md` IS now in English (confirmed: "An OS kernel based on
Zircon with Linux compatibility"). The `docs/README_EN.md` is the older legacy
English version. Its link label calling README.md "Chinese" is stale. The
board- specific docs (D1, C910, fu740, visionfive) and `for-developers.md` are
still in Chinese.
See [#92](https://github.com/andrewdavidmackenzie/zCore/issues/92).


developer guidelines, and hardware-specific deployment guides (primarily in
Chinese).

**Key files:**
- `README_EN.md` -- Authoritative user-facing guide
- `for-developers.md` -- Developer conventions and policies
- `porting-rv64.md` -- RISC-V porting log
- `README-D1.md`, `README-C910.md`, `README-fu740.md`, `README-visionfive.md`
  -- Board-specific deployment
- `structure.svg` -- Architecture diagram

**Status:** Actively used as reference documentation.

---

### `scripts/` -- Build and Test Scripts

**Purpose:** Shell scripts for boot testing, libc testing, and Zircon prebuilt
generation.

**Key files:**
- `boot-test.sh` -- Boot smoke test (QEMU launch, wait for shell prompt,
  poweroff)
- `libc-test.sh` -- Run musl libc-test suite in QEMU, report pass/fail counts
- `gen-prebuilt.sh` -- Generate Zircon prebuilts from Fuchsia source

TODO Can you explain that about Zircon prebuilts more, 
and what Fuschia source is used from where?
  > `gen-prebuilt.sh` runs INSIDE a standard Fuchsia
  > source checkout (`fuchsia.googlesource.com`). It
  > configures a Fuchsia `bringup` build, applies
  > patches, builds, and extracts: `userboot.so`
  > (initial process), `libzircon.so` (vDSO),
  > `bringup.zbi` (boot image). These are REAL
  > Fuchsia binaries, just patched to work with
  > zCore's slightly different syscall ABI.

The ABI difference: real Zircon uses hardware trap instructions for syscalls
(`syscall` on x86_64, `svc` on aarch64). zCore instead uses an indirect jump
through a function pointer (`zcore_syscall_entry`) that zCore patches at load
time to point to its own syscall handler. This allows the same userspace
binaries to work with a different kernel implementation.

- `zcore.patch` / `zircon-libos.patch` -- Fuchsia source patches for zCore
  compatibility

zCore reimplements the Zircon kernel but reuses real Fuchsia userspace
binaries. The patches: (1) Replace x86_64 `syscall` instruction with an
indirect jump through `zcore_syscall_entry` (a function pointer zCore patches
at load time). Same for aarch64 `svc #0` -> indirect `blr`. (2) Fix VMAR
address reservation calculations that assume non-zero base (zCore's VMAR has
base=0, causing underflow). (3) The libos patch additionally modifies Zircon
libos syscall stubs for function-call convention. Without these patches,
Fuchsia userspace would use the wrong syscall mechanism.


**Status:** Actively used in CI (`boot-test.sh`, `libc-test.sh`).

---

### `prebuilt/` -- Prebuilt Firmware

**Purpose:** Pre-built firmware binaries for booting on various platforms.

**Contents:**
- `firmware/aarch64/` -- UEFI bootloader EFI, Boot.json, QEMU_EFI.fd
- `firmware/riscv/` -- OpenSBI firmware, DTBs, FIT sources for C910, D1, FU740,
  VisionFive

**Status:** Actively used. Required for QEMU boot (QEMU_EFI.fd) and physical
hardware deployment.

---

### `rootfs/` -- Root Filesystem

**Purpose:** User-space filesystem trees packed into disk images for running
inside zCore.

**Contents:**
- `aarch64/` -- busybox + 34 symlinked utilities + musl dynamic linker + libc-
  test binaries

All symlinks point to `busybox`: cat, cp, echo, false, grep, gzip, halt, kill,
ln, ls, mkdir, mv, pidof, ping, ping6, poweroff, printenv, ps, pwd, reboot, rm,
rmdir, sh, sleep, stat, tar, timeout, touch, true, uname, usleep, watch. Source
is cloned from `https://git.busybox.net/busybox.git` (official busybox repo)
into `ignored/origin/repos/ busybox/`. Built binaries cached at
`ignored/target/{arch}/busybox/busybox`.


Yes, exactly. The SFS image has root `/` with: `/bin/busybox` (the binary),
`/bin/sh`, `/bin/ls`, etc. (symlinks to busybox), `/lib/ld-musl- {arch}.so.1`
(musl C library). When mounted as rootfs, `/bin/sh` is the shell. Optionally
`/bin/libc-test/` contains test executables.


`rootfs/` IS an output-only directory (gitignored). Built by `cargo rootfs`
which calls `LinuxRootfs::make()` in `xtask/src/linux/mod.rs`: (1) downloads
musl cross-toolchain, (2) clones busybox, runs `make defconfig`, patches
.config for CONFIG_STATIC=y, (3) cross-compiles busybox with musl, strips it,
(4) creates rootfs/{arch}/bin/ and lib/, (5) copies busybox, musl libc, (6)
creates symlinks from a hardcoded list of 31 utility names at
xtask/src/linux/mod.rs:67-72. The utility list is the definition.


These are Linux-only (ELF binaries linked against musl libc). Zircon mode does
NOT use rootfs at all -- it boots from a ZBI containing Fuchsia- format
binaries. The two formats are incompatible (different ABIs, different syscall
interfaces). Running both side-by-side would require the dual- personality
kernel discussed earlier.


Yes, `uutils/coreutils` (Rust reimplementation of GNU coreutils) could be
cross-compiled with musl and included. They provide extensive test suites.
However, they require more syscalls than busybox (e.g., advanced file
operations, xattrs, ACLs) so some tests would fail initially. This would be an
excellent way to discover missing syscall implementations.
See [#93](https://github.com/andrewdavidmackenzie/zCore/issues/93).


To add a new utility: 1. Cross-compile it: `aarch64-linux-musl-gcc -o myutil
myutil.c -static` (or `cargo build
--target aarch64-unknown-linux-musl`) 2. Copy the binary to
`rootfs/aarch64/bin/` 3. Rebuild the image: `cargo image --arch aarch64` 4.
Run: `cargo qemu --arch aarch64`, then at the shell prompt: `/bin/myutil` For
permanent inclusion, add the binary name to the symlink list in
`xtask/src/linux/mod.rs:67` (if it's a busybox applet) or add a copy step to
`LinuxRootfs::make()`.


- `riscv64/` -- busybox + 29 symlinked utilities + musl dynamic linker

The utility LIST is arch-independent (same busybox applets), but the BINARIES
must be compiled per-architecture (different ISA, different musl libc). The
list is already defined once in `xtask/src/linux/mod.rs:67-72`; `cargo rootfs
--arch {arch}` builds for the specified target. The per-arch rootfs/
directories exist because each contains arch-specific binaries.


Resurrecting x86_64 support is tracked in
[#94](https://github.com/andrewdavidmackenzie/zCore/issues/94).
Currently blocked on: (1) rboot submodule needs
updating, (2) x86_64 QEMU launch in xtask is marked
TODO, (3) no x86_64 rootfs/busybox build path.


**Status:** Actively used. Built by `cargo rootfs`, packed by `cargo image`,
used by `cargo qemu`.

---

### `tools/` -- Docker Development Environment

**Purpose:** Dockerfile and scripts for building a containerized zCore
development environment (Ubuntu 20.04, QEMU, Rust).

Agreed. Suggested reorganization: `tools/docker/` (current tools/docker/),
`tools/scripts/` (current scripts/). Would clean up the root directory. Low
priority
but straightforward. See [#95](https://github.com/andrewdavidmackenzie/zCore/issues/95).


**Status:** Moderately used. The Dockerfile is somewhat dated but functional.
CI does not use Docker.

---

### `ignored/` -- Build Artifacts (gitignored)

**Purpose:** Downloaded/built artifacts: cross-compilation toolchains, busybox
builds, firmware.

**Structure:**
- `origin/` -- Downloaded source archives and repos (busybox, musl-cross)
- `target/` -- Built artifacts per architecture

**Status:** Actively used (auto-populated by xtask). Not in version control.

---

## Dependency Tree
