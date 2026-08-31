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
| 9    | Launch userspace     | Linux: `linux::run()`    |
|      |                      | Zircon: `run_userboot()` |
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
        let proc = zcore_loader::linux::run(
            args, envs, rootfs
        );
    } else if #[cfg(feature = "zircon")] {
        let zbi = fs::zbi();
        let proc = zcore_loader::zircon
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

```
┌─────────────────────────────────────────────────────┐
│  Fuchsia userspace                                  │
│  (drivers, filesystems, component manager,          │
│   netstack, package manager, etc.)                  │
│  All Fuchsia functionality above the kernel runs    │
│  here as userspace services communicating via        │
│  channels and FIDL.                                 │
├─────────────────────────────────────────────────────┤
│  userboot (first userspace process)                 │
│  Receives handles from kernel, unpacks the ZBI      │
│  bootfs, loads the next program (bootsvc or         │
│  component_manager), passes handles onward, exits.  │
│  ~500 lines of C in real Fuchsia.                   │
├─────────────────────────────────────────────────────┤
│  vDSO (libzircon.so)                                │
│  Kernel-provided shared library mapped into every   │
│  userspace process. Contains syscall entry stubs    │
│  (svc on aarch64, syscall on x86_64) and read-only  │
│  kernel constants (ticks_per_second, cache sizes).  │
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

`run_userboot()` in `loader/src/zircon.rs` implements the kernel side:

```
run_userboot(zbi_data, cmdline)
  │
  ├── 1. Parse userboot.so as ELF, map into new process VMAR
  ├── 2. Parse libzircon.so (vDSO) as ELF, map after userboot
  ├── 3. [libos mode] Patch vDSO syscall entry to point to
  │      kernel_hal::context::syscall_entry (function call,
  │      not hardware trap)
  ├── 4. Create ZBI VMO from boot image data
  ├── 5. Set up 32 KiB user stack
  ├── 6. Create channel pair (user_channel, kernel_channel)
  ├── 7. Pack 15 handles onto kernel_channel:
  │        [0]  PROC_SELF          Process handle
  │        [1]  VMARROOT_SELF      Root VMAR
  │        [2]  ROOTJOB            Root job
  │        [3]  ROOTRESOURCE       Root resource
  │        [4]  ZBI                ZBI VMO
  │        [5-7] VDSO              vDSO VMOs (3 variants)
  │        [8]  CRASHLOG           Crash log VMO
  │        [9]  COUNTER_NAMES      Kernel counter descriptors
  │        [10] COUNTERS           Kernel counter arena
  │        [11-14] INSTRUMENTATION Profiling VMOs (stubs)
  ├── 8. Write VdsoConstants at fixed offset in vDSO VMO
  └── 9. Start thread at userboot entry point with user_channel
```

userboot then runs in userspace:
1. Reads the 15 handles from its channel
2. Parses the ZBI to find the bootfs
3. Loads the next program from bootfs (typically `bootsvc`)
4. Creates a new process, maps ELF segments, passes handles via channel
5. Exits

### Required Binaries

Zircon mode requires three prebuilt artifacts at `prebuilt/zircon/{arch}/`:

| File | Embedded | Purpose |
|------|----------|---------|
| `userboot.so` | Compile-time (`include_bytes!`) | First userspace process |
| `libzircon.so` | Compile-time (`include_bytes!`) | vDSO with syscall stubs |
| `bringup.zbi` | Runtime (loaded from disk/initramfs) | Boot image with bootfs |

These are currently **not present** in the repo. See issue #86 for restoration
plans, #121 for a Rust-native replacement approach, and #122 for updating the
Fuchsia source patches.

### The vDSO and Syscall ABI

The vDSO (virtual Dynamic Shared Object) is how userspace calls into the
kernel. It is a small shared library that the kernel maps into every process
at a random address. Userspace calls functions like `zx_channel_create()`
which are thin assembly stubs in the vDSO:

**Bare-metal mode:** The stubs use hardware trap instructions (`svc #0` on
aarch64, `syscall` on x86_64) that transfer control to the kernel's trap
handler.

**LibOS mode:** The stubs use indirect function calls through a pointer
(`zcore_syscall_entry`) that zCore patches at load time to point to its own
syscall handler. This is necessary because in libos mode, the kernel and
userspace share the same address space.

The vDSO also exports read-only data (`VdsoConstants`) including CPU count,
ticks-per-second, cache line sizes, and physical memory size. This lets
userspace read kernel data without a syscall (e.g., `zx_ticks_get()`).
