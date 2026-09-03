# Boot Process

This document describes the full boot sequence for each platform supported by
zCore. All platforms converge at `primary_main()` in `zCore/src/main.rs:39`.

For architecture overview, see
[architecture.md](architecture.md).

---

## AArch64 Boot Sequence

### Prerequisites

QEMU loads the kernel ELF via the `-kernel` flag, sets the CPU to EL1
(supervisor mode) with MMU off, places a DTB pointer in `x0`, and jumps to
`_boot`.

### Boot Flow

```
_boot (boot.s:34) ............. MMU off, physical addrs
  |
  +-- Zero 4 page tables (boot.s:40-59)
  +-- Populate L0/L1 page table entries (boot.s:61-122)
  |     L1: 3 x 1 GiB block mappings:
  |       [0] 0x00..0x40000000 (device, MAIR=0x04)
  |       [1] 0x40..0x80000000 (normal, MAIR=0xFF)
  |       [2] 0x80..0xC0000000 (normal, MAIR=0xFF)
  |     Identity-mapped (TTBR0) AND high-mapped (TTBR1)
  +-- Enable FP/SIMD (boot.s:126-128)
  +-- Configure MAIR, TCR, TTBR0/TTBR1 (boot.s:135-183)
  +-- Flush TLB (boot.s:186-188)
  +-- Enable MMU + I-cache + D-cache (boot.s:191-196)
  |
  v
_start_virtual (boot.s:214) ... MMU on, virtual addrs
  |
  +-- Zero BSS (boot.s:220-228)
  +-- Set boot stack (32 KiB) (boot.s:231-233)
  +-- Restore DTB pointer (boot.s:236)
  |
  v
rust_main (entry.rs:19) ........ Rust code begins
  |
  +-- Build KernelConfig:
  |     uart_base: 0x0900_0000
  |     gic_base:  0x0800_0000
  |     phys_to_virt_offset: 0xffff_0000_0000_0000
  +-- Save offset in Once<usize> (consts.rs:8)
  |
  v
primary_main (main.rs:39) ..... Common boot path
```

### Key Addresses

| Constant            | Value                   |
|---------------------|-------------------------|
| Virtual base        | `0xffff_0000_4008_0000` |
| Physical base       | `0x4008_0000`           |
| Phys-to-virt offset | `0xffff_0000_0000_0000` |
| UART (PL011)        | `0x0900_0000`           |
| GIC (GICv2)         | `0x0800_0000`           |
| Boot stack          | 32 KiB                  |

### SMP

Not implemented for AArch64. `secondary_main()` is excluded via
`#[cfg(not(target_arch = "aarch64"))]`.

---

## RISC-V 64 Boot Sequence

### Prerequisites

OpenSBI or QEMU provides the SBI interface. The bootloader/SBI jumps to
`_start` with `a0 = hartid`, `a1 = device_tree_paddr`.

### Boot Flow

```
_start (entry.rs:17) ........... Naked fn, MMU off
  |
  +-- select_stack(hartid) (entry.rs:105)
  |     tp = hartid
  |     sp = BOOT_STACK + (hartid+1) * 128 KiB
  |
  v
primary_rust_main (entry.rs:43)  Rust code begins
  |
  +-- Zero BSS via r0::zero_bss (entry.rs:44-49)
  +-- BOOT_PAGE_TABLE.init() (boot_page_table.rs:14)
  |     Probe kernel physical location
  |     Build Sv39 page table:
  |       - Trampoline: identity-map 1 GiB mega-page
  |         containing paddr_base
  |       - Kernel: map 128 GiB physical -> virtual
  |         at offset (0xffff_ffc0_0000_0000 region)
  +-- BOOT_PAGE_TABLE.launch() (boot_page_table.rs:51)
  |     Set satp = Sv39 | page_table_ppn
  |     jump_higher(offset): relocate sp and ra
  |     Set sstatus.SUM = 1
  |
  +-- Print boot info (entry.rs:64-76)
  +-- boot_secondary_harts() (entry.rs:78)
  |     Walk DTB /cpus/cpu@N nodes
  |     For each non-boot hart: sbi_rt::hart_start()
  +-- Build KernelConfig:
  |     phys_to_virt_offset, dtb_paddr, dtb_size
  |
  v
primary_main (main.rs:39) ...... Common boot path
```

### Key Addresses

| Constant            | Value                     |
|---------------------|---------------------------|
| Virtual base        | `0xffff_ffc0_8020_0000`   |
| Physical base       | Runtime (from linker)     |
| Phys-to-virt offset | `vaddr_base - paddr_base` |
| Stack per hart      | 128 KiB (32 pages)        |
| Max harts           | 5                         |

### SMP

Implemented via SBI HSM extension. `boot_secondary_harts()` walks the DTB,
finds CPU nodes with `status = "okay"`, and calls `sbi_rt::hart_start(hartid,
start_addr, 0)`.

Secondary harts enter `secondary_hart_start()` -> `select_stack()` ->
`secondary_rust_main()` -> `BOOT_PAGE_TABLE.launch()` -> `secondary_main()`.

`secondary_main()` spin-waits on the `STARTED` atomic, then calls
`kernel_hal::secondary_init()` and enters the executor loop.

---

## x86_64 Boot Sequence

### Prerequisites

The `rboot` UEFI bootloader handles all early setup: paging, memory map
discovery, ACPI RSDP location, framebuffer initialization. It passes all
information via a `BootInfo` struct.

### Boot Flow

```
rboot (UEFI bootloader) ....... Sets up paging, etc.
  |
  v
_start(boot_info) (entry.rs:5)  Receives BootInfo
  |
  +-- Build KernelConfig from BootInfo:
  |     cmdline, initramfs, memory_map,
  |     physical_memory_offset, graphic_info,
  |     acpi2_rsdp_addr, smbios_addr,
  |     ap_fn: secondary_main
  |
  v
primary_main (main.rs:39) ...... Common boot path
```

### Key Addresses

All addresses come from `rboot::BootInfo` at runtime. No hardcoded constants
(consts.rs is empty).

### SMP

`KernelConfig` includes `ap_fn: crate::secondary_main`. The HAL's x86_64
implementation uses `x86-smpboot` to start Application Processors, calling this
function pointer on each AP.

---

## LibOS Boot Sequence

### Prerequisites

None. Runs as a regular userspace process on a host OS (Linux or macOS). No
bootloader, no assembly, no page tables.

### Boot Flow

```
main() (entry.rs:2) ............ Normal Rust main()
  |
  v
primary_main(KernelConfig) ..... KernelConfig = ()
```

`KernelConfig` is an empty unit struct. The `#![no_std]` attribute is
conditionally removed for libos mode.

For full LibOS details, see [libos.md](libos.md).

---

## Common Post-Boot Path

All platforms converge at `primary_main()` in `zCore/src/main.rs:39`:

| Step | Function             | Description              |
|------|----------------------|--------------------------|
| 1    | `logging::init()`    | Init log framework       |
| 2    | `memory::init()`     | Seed buddy allocator     |
|      |                      | with 2 MiB static block  |
| 3    | `primary_init_early` | Store config + kernel    |
|      | `(config, &handler)` | handler in globals       |
| 4    | `boot_options()`     | Parse cmdline            |
|      |                      | (`KEY=value:KEY=value`)  |
| 5    | `set_max_level()`    | Set log level from       |
|      |                      | `LOG=` option            |
| 6    | `insert_regions`     | Register physical memory |
|      | `(free_pmem)`        | with buddy allocator     |
| 7    | `primary_init()`     | Full HAL initialization  |
| 8    | `STARTED.store`      | Signal secondary cores   |
|      | `(true)`             | to proceed               |
| 9    | Launch userspace     | Linux: `linux::run()`     |
|      |                      | Zircon: `run_userstart()` |
| 10   | `wait_for_exit`      | Wait for root process    |

### Memory Allocator

On aarch64/riscv64, `memory.rs` provides a unified `BuddyAllocator` (from
`customizable-buddy`) that serves as both `#[global_allocator]` (heap) and
physical frame allocator. Bootstrapped with a 2 MiB static array, then expanded
with physical memory regions from the HAL.

On x86_64, `memory_x86_64.rs` uses two separate allocators:
`buddy_system_allocator` for the heap and `bitmap-allocator` for frame
tracking.

### Personality Launch

The personality is selected at compile time:

```rust
// main.rs:49-65
cfg_if! {
    if #[cfg(feature = "linux")] {
        let rootfs = fs::rootfs();
        let proc = linux_loader::linux::run(
            args, envs, rootfs
        );
    } else if #[cfg(feature = "zircon")] {
        let zbi = fs::zbi();
        let proc = zircon_loader::zircon
            ::run_userboot(zbi, cmdline);
    }
}
```

Both `linux` and `zircon` features cannot be enabled simultaneously (panics at
compile time if both set).

---

## Zircon Boot Protocol

When zCore boots in Zircon mode, it must launch the Fuchsia userspace. This
involves several components that bridge the kernel and userspace worlds.

### Architecture Overview

```text
┌─────────────────────────────────────────────────────┐
│  petal test programs / Fuchsia userspace             │
│  Simple test programs using zircon-abi syscall       │
│  wrappers, or full Fuchsia services (drivers,       │
│  filesystems, component manager, etc.)              │
├─────────────────────────────────────────────────────┤
│  userstart (first userspace process)                │
│  Kernel-generated code that writes a debug message  │
│  and exits. Future: loads programs from ZBI bootfs. │
│  Replaces Fuchsia's userboot (see #121).            │
├═════════════════════════════════════════════════════╡
│  KERNEL  (zCore -- this project)                    │
│  Zircon kernel objects: Process, Thread, VMO,       │
│  Channel, VMAR, Port, Futex, etc.                   │
│  Syscall handlers (zCore/zircon-syscall/)            │
│  HAL, drivers, async executor                       │
└─────────────────────────────────────────────────────┘
```

**Key insight:** Fuchsia is a microkernel OS. Everything above the kernel is
userspace -- device drivers, filesystems, networking, the component framework.
zCore replaces the Zircon kernel. If it implements the same syscalls with the
same ABI, real Fuchsia userspace programs run on it unchanged.

### The Bootstrap Sequence

`run_userstart()` in `zCore/zircon-loader/src/zircon.rs` implements the
kernel side. `run_userboot()` is a backward-compatible alias.

```text
run_userstart(zbi_data, cmdline)
  │
  ├── 1. Load userstart ELF (embedded at compile time)
  ├── 2. Parse ELF headers, map segments into process VMAR
  ├── 3. Create stub vDSO VMO (placeholder)
  ├── 4. Create ZBI VMO from boot image data
  ├── 5. Set up 32 KiB user stack
  ├── 6. Create channel pair (user_channel, kernel_channel)
  ├── 7. Pack 15 handles onto kernel_channel:
  │        [0]  PROC_SELF          Process handle
  │        [1]  VMARROOT_SELF      Root VMAR
  │        [2]  ROOTJOB            Root job
  │        [3]  ROOTRESOURCE       Root resource
  │        [4]  ZBI                ZBI VMO
  │        [5-7] VDSO              vDSO VMOs (stubs)
  │        [8]  CRASHLOG           Crash log VMO
  │        [9]  COUNTER_NAMES      Kernel counter descriptors
  │        [10] COUNTERS           Kernel counter arena
  │        [11-14] INSTRUMENTATION Profiling VMOs (stubs)
  └── 8. Start thread at userstart ELF entry point with user_channel
```

Userstart then runs in userspace (see `zCore/userstart/src/main.rs`):
1. Reads the 15 bootstrap handles via `zx_channel_read`
2. Maps the ZBI VMO and parses the bootfs to find the init program
3. Creates a new process, maps the init code, creates a stack
4. Forwards selected bootstrap handles to init via a new channel
5. Starts the init process and waits for it to exit
6. Shuts down when init terminates

The init program (e.g., petal's `hello`) receives a startup handle
(channel) containing forwarded bootstrap handles. Petal programs
define `pub fn main()` with the petal runtime providing `_start`.

### Syscall ABI

The `zircon-abi` crate (`zCore/zircon-abi/`) defines the Zircon syscall ABI
for userspace programs:

- **Syscall numbers** matching `zx-syscall-numbers.h` from Zircon
- **Error codes** matching `zx_status_t`
- **Inline syscall wrappers** using `svc #0` (aarch64), `syscall` (x86_64),
  or `ecall` (riscv64) instructions
- **Safe wrappers** like `debug_write(&[u8])`, `debug_print(&str)`,
  `process_exit(i64)` -- no `unsafe` needed at call site
- **ZBI format** parsing and construction

Userstart and petal programs use these inline wrappers instead of a vDSO
shared library. The syscall instruction traps into the kernel, which
dispatches via the trap loop in `zCore/zircon-loader/src/zircon.rs`.

### Legacy: Fuchsia Prebuilt Binaries

The original approach required prebuilt Fuchsia binaries (`userboot.so`,
`libzircon.so`, `bringup.zbi`) generated from the Fuchsia source tree.
This dependency has been replaced by the self-contained userstart approach.
For running real Fuchsia userspace programs on zCore (ABI compatibility
testing), see #122.
