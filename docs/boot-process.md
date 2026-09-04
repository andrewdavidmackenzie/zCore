# Boot Process

This document describes how zCore boots across all supported platforms and
modes. For architecture overview, see [architecture.md](architecture.md).
For the Zircon userspace bootstrap protocol, see [userstart.md](userstart.md).

---

## Overview

zCore supports three execution modes and three CPU architectures:

| Mode              | Description                                       |
|-------------------|---------------------------------------------------|
| **QEMU**          | Emulated hardware, the primary development target |
| **Real hardware** | Physical boards (riscv64 only currently)          |
| **LibOS**         | Runs as a host OS process (Linux/macOS)           |

| Architecture | QEMU               | Real hardware                         | LibOS        |
|--------------|--------------------|---------------------------------------|--------------|
| **aarch64**  | Active             | Planned (#11)                         | Broken (#80) |
| **riscv64**  | Active             | Supported (D1, C910, FU740, StarFive) | Broken (#80) |
| **x86_64**   | Active (BIOS boot) | Not yet supported                     | Broken (#80) |

After platform-specific initialization, all paths converge at
`primary_main()` in `zCore/src/main.rs`, which branches into either
Linux or Zircon personality mode.

---

## Boot Modes

### QEMU (Emulated Hardware)

QEMU loads the kernel directly and starts execution. No user-facing
firmware interaction is needed.

**aarch64:** QEMU loads the ELF via `-kernel`, sets CPU to EL1 with MMU
off, and jumps to the ELF entry point. QEMU places a DTB in memory but
the DTB pointer in x0 is currently 0 -- DTB parsing is not yet
implemented (#136). The first zCore instruction is in `boot.s` (assembly).

**riscv64:** QEMU loads a raw binary via `-kernel` with `-bios default`
(OpenSBI). OpenSBI runs at M-mode, initializes hardware, then jumps to
the kernel at S-mode. The first zCore instruction is in `entry.rs`
(inline assembly in a naked function).

**x86_64:** QEMU boots via BIOS using the `bootloader` crate (v0.11).
The bootloader handles page table setup, physical memory mapping, and
passes `BootInfo` to the kernel. A boot image is created by the
`tools/x86-bootimage` helper tool. The kernel boots fully but requires
a rootfs to run userspace programs.

### Real Hardware

Physical boards require platform-specific firmware to initialize hardware
and load the kernel. The firmware brings the CPU to a known state and
jumps to the kernel entry point. Board-specific firmware files are in
`firmware/`.

Each architecture has a different firmware convention:
- **aarch64:** GPU firmware (Raspberry Pi) or UEFI (server boards)
- **riscv64:** SBI firmware (OpenSBI or vendor-specific) at M-mode
- **x86_64:** UEFI firmware with a bootloader application

See the platform-specific sections below for details on each board.

#### riscv64 boards

riscv64 boards use SBI firmware that runs at M-mode and provides the
Supervisor Binary Interface. The kernel runs at S-mode.

| Board             | Firmware                                            | Feature flag      |
|-------------------|-----------------------------------------------------|-------------------|
| Allwinner D1      | `firmware/riscv/d1_fw_payload.elf`                  | `board-d1`        |
| T-HEAD C910 Light | `firmware/riscv/c910_fw_dynamic.bin`                | `board-c910light` |
| SiFive FU740      | OpenSBI + `firmware/riscv/hifive-unmatched-a00.dtb` | `board-fu740`     |
| StarFive          | OpenSBI + `firmware/riscv/starfive.dtb`             | --                |

#### aarch64 boards

See the "AArch64 (Raspberry Pi)" section below. Not yet implemented (#11).

#### x86_64 hardware

See the "x86_64 (Real Hardware / UEFI Laptop)" section below.
QEMU works via BIOS boot; real hardware needs UEFI (#148).

### LibOS (Host OS Process)

zCore is compiled as a regular userspace executable with `std`. The host
OS loader starts it like any normal program. No assembly, no page tables,
no bootloader. `main()` calls `primary_main()` directly.

---

## Platform Boot Sequences

### AArch64 (QEMU)

```text
QEMU loads ELF via -kernel, sets EL1, MMU off
  (x0 should be DTB ptr but is currently 0 -- see #136)
  │
  v
_boot (boot.s) ................ Assembly, physical addresses
  ├── Save DTB pointer (x0 -> x20)
  ├── Zero 4 page table pages
  ├── Build L0/L1 page tables (1 GiB block mappings):
  │     Identity: 0x00..0xC0000000 (device + 2 GiB RAM)
  │     High:     0xFFFF_0000_0000_0000..0xFFFF_0000_C000_0000
  ├── Enable FP/SIMD
  ├── Configure MAIR, TCR, TTBR0/TTBR1
  ├── Enable MMU + caches
  │
  v
_start_virtual (boot.s) ....... Virtual addresses
  ├── Zero BSS
  ├── Set up 32 KiB boot stack
  │
  v
rust_main (entry.rs) .......... Rust code begins
  ├── Build KernelConfig (hardcoded QEMU virt constants)
  │
  v
primary_main (main.rs)
```

**Key addresses:**
- Physical base: `0x4008_0000`
- Virtual base: `0xFFFF_0000_4008_0000`
- UART (PL011): `0x0900_0000`
- GIC (GICv2): `0x0800_0000`

**First zCore instruction:** `mov x20, x0` (save DTB pointer) in `boot.s`

**No SMP:** Secondary core boot is not implemented for aarch64.

---

### RISC-V 64 (QEMU / Default)

```text
QEMU + OpenSBI (-bios default), S-mode, MMU off
  a0 = hart ID, a1 = DTB physical address
  │
  v
_start (entry.rs) ............. Naked fn, inline asm
  ├── select_stack(): set tp = hartid, sp = per-hart stack
  │
  v
primary_rust_main (entry.rs) .. Rust code begins
  ├── Zero BSS
  ├── Build Sv39 page table:
  │     Trampoline: identity-map 1 GiB containing kernel
  │     High: 128 GiB physical -> virtual offset
  ├── Enable MMU (write satp), jump to virtual addresses
  ├── Validate DTB with dtb-walker
  ├── Boot secondary harts via SBI HSM
  ├── Build KernelConfig (dtb_paddr, phys_to_virt_offset)
  │
  v
primary_main (main.rs)
```

**Key addresses:**
- Virtual base: `0xFFFF_FFC0_8020_0000` (default, set by `build.rs`)
- Physical base: detected at runtime from linker symbols

**First zCore instruction:** `call select_stack` in `_start` naked function

**SMP:** Secondary harts boot via SBI HSM `hart_start()`, entering
`secondary_hart_start` -> `secondary_rust_main` -> `secondary_main`.

---

### RISC-V 64 (C910 Light Board)

Uses `boot.asm` (pure assembly) instead of the Rust-based boot for the
default path. Activated by `feature = "board-c910light"`.

```text
SBI firmware (c910_fw_dynamic.bin), S-mode, MMU off
  │
  v
_start (boot.asm) ............ Assembly
  ├── Disable interrupts (sie = 0)
  ├── Zero BSS
  ├── Build Sv39 page tables (hardcoded 1 GiB mega-pages)
  ├── Enable MMU (write satp)
  ├── Set per-hart stack
  │
  v
primary_rust_main (entry64.rs)  Rust code begins
  ├── Boot secondary harts via SBI
  │
  v
primary_main (main.rs)
```

**First zCore instruction:** `csrw sie, zero` in `boot.asm`

---

### AArch64 (Raspberry Pi) -- Planned

**Status:** Not yet implemented. See #11, #9, #8.

The Raspberry Pi 4 uses a BCM2711 SoC with a Cortex-A72 (same as QEMU
virt). However, the boot process and peripherals differ significantly:

- **Boot:** The Pi's GPU firmware loads `kernel8.img` from the SD card's
  FAT partition, sets up basic hardware, and jumps to the kernel at EL2.
  zCore would need to drop to EL1 (or run at EL2).
- **Peripherals:** BCM283x mini-UART (not PL011 by default), VideoCore
  GPU, BCM interrupt controller (not GIC). New drivers would be needed.
- **DTB:** The GPU firmware generates a DTB and passes it in x0 (same
  convention as QEMU). DTB parsing (#136) would enable auto-discovery.

```text
Pi GPU firmware loads kernel8.img from SD card
  │  EL2, MMU off, x0 = DTB pointer
  v
_boot (boot.s) ................ Same assembly as QEMU path
  ├── (would need EL2 -> EL1 transition)
  ├── Page table setup
  ├── Enable MMU
  v
rust_main (entry.rs) .......... Would need Pi-specific KernelConfig
  v
primary_main (main.rs)
```

**Required work:**
- EL2 to EL1 drop in boot assembly
- BCM283x UART driver (#87)
- BCM interrupt controller driver
- DTB parsing for peripheral discovery (#136)
- SD card / eMMC block device driver

---

### x86_64 (Real Hardware / UEFI Laptop) -- Planned

**Status:** QEMU works via BIOS boot. Real hardware needs UEFI support
which is tracked in #148.

Real hardware requires a UEFI bootloader since modern PCs boot via UEFI.
The `bootloader` crate supports both BIOS and UEFI; enabling UEFI mode
requires the `x86_64-unknown-uefi` Rust target.

```text
UEFI firmware (laptop/PC)
  │  Loads bootloader .efi from ESP
  v
Bootloader (bootloader crate, UEFI mode)
  ├── Load kernel ELF from ESP
  ├── Set up page tables, memory map
  ├── Exit UEFI boot services
  v
_start (entry.rs) ............. Rust code, MMU already on
  v
primary_main (main.rs)
```

**Required work:**
- Enable UEFI mode in the `bootloader` crate (#148)
- ACPI table parsing for device discovery
- Real hardware drivers (NVMe, USB, framebuffer)
- Secure Boot support (optional)

---

### x86_64 (BIOS via bootloader crate)

```text
SeaBIOS (QEMU built-in BIOS)
  │
  v
bootloader crate (v0.11) ...... Multi-stage BIOS bootloader
  ├── Stage 2: real mode -> protected mode
  ├── Stage 3: set up page tables (4-level, NX bit)
  ├── Stage 4: load kernel ELF, map physical memory
  │     Map all physical memory at 0xFFFF_8000_0000_0000
  │     Map kernel at 0xFFFF_FFFF_8000_0000
  ├── Switch to long mode (64-bit)
  │
  v
kernel_main (entry.rs) ........ Rust code, MMU already on
  ├── Translate bootloader_api::BootInfo to KernelConfig
  │     (memory regions, framebuffer, RSDP, ramdisk)
  │
  v
primary_main (main.rs)
```

**Boot image creation:** The `tools/x86-bootimage` helper tool uses the
`bootloader` crate to create a BIOS-bootable disk image from the kernel
ELF. Run via xtask: `cargo qemu --arch x86_64`.

**Key addresses:**
- Kernel base: `0xFFFF_FFFF_8000_0000` (upper 2 GB, kernel code model)
- Physical memory: `0xFFFF_8000_0000_0000` (direct map)

**First zCore instruction:** First Rust statement in `kernel_main` (no
assembly needed -- bootloader set up page tables and long mode).

**SMP:** Not currently implemented (x86-smpboot dependency removed).

**QEMU flags:** `-machine q35 -cpu qemu64,+fsgsbase` (fsgsbase required
by the trapframe crate).

---

### LibOS (Host OS Process)

```text
Host OS loads zCore as a normal executable
  │
  v
main (entry.rs) ............... Standard Rust main()
  │
  v
primary_main (main.rs) ........ KernelConfig = () (empty)
```

**First zCore instruction:** `crate::primary_main(kernel_hal::KernelConfig)`

No assembly, no page tables, no bootloader. The host OS provides all
hardware abstraction. HAL uses `mmap`, `tmpfile`, `async-std`, and SDL
for mock devices.

**Status:** Currently broken for both Linux and Zircon modes (#80).

---

## Common Post-Boot Path

All platforms converge at `primary_main()` in `zCore/src/main.rs`:

| Step | Function                               | Description                                  |
|------|----------------------------------------|----------------------------------------------|
| 1    | `logging::init()`                      | Init log framework                           |
| 2    | `memory::init()`                       | Seed buddy allocator with 2 MiB static block |
| 3    | `primary_init_early(config, &handler)` | Store config, arch-specific early init       |
| 4    | `boot_options()`                       | Parse cmdline (`KEY=value:KEY=value`)        |
| 5    | `set_max_level()`                      | Set log level from `LOG=` option             |
| 6    | `insert_regions(free_pmem)`            | Register physical memory with allocator      |
| 7    | `primary_init()`                       | Full HAL initialization                      |
| 8    | `STARTED.store(true)`                  | Signal secondary cores to proceed            |
| 9    | Launch userspace                       | Linux or Zircon (see below)                  |
| 10   | `wait_for_exit`                        | Wait for root process                        |

---

## Personality Launch

The personality is selected at compile time via mutually exclusive features:

### Linux Mode (`--features linux`)

```rust
let rootfs = fs::rootfs();  // SFS image, initrd, or host FS
let proc = linux_loader::linux::run(args, envs, rootfs);
```

The kernel directly loads the init program (busybox) from a filesystem,
parses the ELF, maps it into a process, and starts it. All Linux syscall
handling (VFS, signals, networking, etc.) runs in kernel space.

**Rootfs sources by platform:**
- aarch64 QEMU: SFS image on VirtIO block device (`-drive`)
- riscv64 QEMU: SFS image as initrd (`-initrd`)
- LibOS: Host filesystem passthrough (HostFS)

### Zircon Mode (`--features zircon`)

```rust
let zbi = fs::zbi();  // embedded at compile time
let proc = zircon_loader::zircon::run_userboot(zbi, &options.cmdline);
// run_userboot is a backward-compatible alias for run_userstart
```

The kernel loads the **userstart** ELF (embedded at compile time) as the
first userspace process. Userstart then runs in userspace making Zircon
syscalls to bootstrap the system:

```text
Kernel
  ├── Loads userstart ELF, maps into process
  ├── Packs 15 bootstrap handles into a channel
  ├── Starts userstart thread
  │
  v
Userstart (userspace, zCore/userstart/)
  ├── Reads bootstrap handles via zx_channel_read
  ├── Maps ZBI VMO, parses bootfs
  ├── Finds init program (e.g., bin/hello)
  ├── Creates child process, maps code, creates stack
  ├── Forwards handles to init via a new channel
  ├── Starts init and waits for it to exit
  │
  v
Init program (userspace, petal/)
  ├── main() runs
  ├── Exits via zx_process_exit
  │
  v
Userstart exits -> kernel shuts down
```

**ZBI source:** Embedded at compile time via `include_bytes!(env!("PETAL_ZBI"))`.
Runtime ZBI loading via DTB initrd is tracked in #136.

---

## Pre-primary_main Setup Comparison

| Setup                 | aarch64              | riscv64                  | x86_64            | LibOS         |
|-----------------------|----------------------|--------------------------|-------------------|---------------|
| **Page tables**       | Assembly (1G blocks) | Rust (Sv39 mega-pages)   | bootloader crate  | Host OS       |
| **MMU enable**        | Assembly             | Rust (`satp`)            | Already on (UEFI) | Already on    |
| **BSS zeroing**       | Assembly             | Rust (`r0::zero_bss`)    | bootloader        | Host OS       |
| **Stack setup**       | Assembly (32 KiB)    | Naked fn (32 pages/hart) | bootloader        | Host OS       |
| **FP/SIMD**           | Assembly             | N/A                      | UEFI enables      | Host OS       |
| **SMP boot**          | None                 | SBI HSM                  | AP fn pointer     | None          |
| **DTB parsing**       | Not yet (#136)       | `dtb-walker` crate       | N/A (ACPI)        | N/A           |
| **First instruction** | `mov x20, x0`        | `call select_stack`      | Rust statement    | Rust `main()` |
