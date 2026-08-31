# zCore Project Architecture

This document describes the architecture of the zCore project, covering all
workspace crates, their purposes, dependencies, and key design decisions.

For detailed boot sequence information, see [boot-process.md](boot-process.md).
For HAL architecture details, see [hal-design.md](hal-design.md). For LibOS
mode, see [libos.md](libos.md).

## Overview

zCore is a Rust reimplementation of the Zircon microkernel (from Google's
Fuchsia OS) that also supports running Linux userspace programs. It targets
three CPU architectures (aarch64, riscv64, x86_64) and can run both bare-metal
and as a "library OS" on a host system.

The project is organized as a Cargo workspace with 12 member crates plus 2
excluded standalone projects and 2 git submodules.

---

## Workspace Member Crates

These are the Rust crates listed in the root `Cargo.toml` workspace `members`
array.

### `zCore/` -- Main Kernel Binary

**Purpose:** The top-level kernel binary that ties everything together. This is
the entry point for kernel execution -- it boots the hardware, initializes
memory, sets up the HAL, and launches either a Linux or Zircon userspace.

QEMU acts as the bootloader for aarch64: it loads the kernel ELF, sets the CPU
to EL1 (supervisor), points x0 at a DTB, and jumps to the entry with MMU off.
`boot.s` then does pre-Rust kernel setup: builds 4 boot page tables (1 GiB
block mappings), enables FP/SIMD, configures MAIR/TCR/TTBR, enables the
MMU+caches, zeroes .bss, sets up a 32 KiB stack, then calls `rust_main`. So:
QEMU/bootloader gets the CPU running; boot.s gets the MMU and memory
environment ready for Rust.


The "Zircon userspace" is the Fuchsia **userboot** program -- the first process
in Fuchsia's boot sequence. zCore does NOT include Zircon userspace source; it
uses **prebuilt binaries** generated from the real Fuchsia source tree (via
`tools/scripts/gen-prebuilt.sh`). The binaries are: `userboot.so` (initial process),
`libzircon.so` (vDSO), and `bringup.zbi` (boot image with bootfs). These are
expected at `prebuilt/zircon/{arch}/` but are NOT currently present in the
repo. Without them, Zircon mode cannot boot. The loader code is at
`loader/src/zircon.rs`.


**Key responsibilities:**
- Platform-specific boot assembly and entry points (per-arch `platform/`
  subdirs)

The page table population and register writes could be done in Rust via
`core::arch::asm!` and `#[naked]` functions, but the first few instructions
(set stack, branch) must be assembly because Rust assumes a stack exists. The
current approach (`global_asm!(include_str!("boot.s"))`) is pragmatic and
standard across Rust OS projects. Reducing the assembly is feasible;
eliminating it entirely on aarch64 is not practical.


- Global heap allocator and physical frame allocator
- `KernelHandler` implementation bridging the HAL to the memory allocator

Agreed, it's vague. The trait has 4 methods: `frame_alloc()`,
`frame_alloc_contiguous()`, `frame_dealloc()`, and `handle_page_fault()`. It's
a callback interface from HAL into the kernel (dependency inversion). Better
names would be `KernelCallbacks`, `HalToKernelBridge`, or `KernelServices`.
Consider renaming in a future cleanup issue.


- OS personality selection (Linux vs Zircon, mutually exclusive via features)

In `zCore/src/main.rs:49-65`, a `cfg_if!` block checks `feature = "linux"` vs
`feature = "zircon"` (panics if both). Linux calls `zcore_loader::linux::run()`
with a rootfs; Zircon calls `zcore_loader::zircon::run_userboot()` with a ZBI.
Features are defined in `zCore/Cargo.toml`: `linux = ["zcore-loader/linux",
"linux-object", "rcore-fs", "rcore-fs-sfs"]` and `zircon = ["zcore-
loader/zircon"]`. They are mutually exclusive compile-time choices.


The core reason is that Linux mode pulls in `linux-object` (filesystem,
signals, SysV IPC) and `linux-syscall` as compile-time dependencies. These are
not runtime-loadable modules. Both modes share `zircon-object` underneath, so
the kernel object model IS shared. In theory, if the personality-specific code
(rootfs init, syscall dispatch table, process creation) were moved to
dynamically-selected modules or to userspace servers, both could coexist. The
main blockers: (1) Linux's in-kernel VFS would need to become a userspace
server, (2) the loader would need to support both syscall dispatch tables
simultaneously, (3) the startup path in main.rs would need to launch both
personality servers.
See [#74](https://github.com/andrewdavidmackenzie/zCore/issues/74).


In a pure microkernel, it shouldn't. But zCore has personality-specific code in
the kernel: (1) Linux mode needs in-kernel filesystem (SFS), (2) the loader
creates first processes differently (ELF+auxvec vs userboot+channel+handles),
(3) `handler.rs` unconditionally downcasts to `zircon_object::task::Thread` for
page faults. Both personalities share Zircon objects underneath -- Linux is
implemented ON TOP of Zircon primitives, not alongside. This is architectural
debt from the research/educational origins.


The personality-specific code in `zCore/src/` is small: `fs.rs` (rootfs vs
ZBI), the startup branch in `main.rs`, and boot option parsing in `utils.rs`.
These could be abstracted behind a trait like `OsPersonality::start()`. The
real blocker is that both personalities share `zircon-object` as the common
kernel object model -- Linux extends it via `ProcessExt`/`ThreadExt`. A truly
clean split would require defining a personality-neutral kernel object
interface, which would be a major refactor. Running both simultaneously is not
currently possible. See [#75](https://github.com/andrewdavidmackenzie/zCore/issues/75).


- Filesystem initialization and root process launch

Real Zircon has **no filesystem** -- it's a pure microkernel. Fuchsia runs
filesystem servers in userspace: Minfs, Blobfs, Memfs, Fxfs, accessed via FIDL
IPC over channels. zCore's Zircon mode matches this: `fs.rs` just returns raw
ZBI bytes, no FS. Linux mode has in-kernel SFS (Simple File System) from rcore-
fs, plus HostFS in libos mode. The in-kernel FS is a pragmatic shortcut for
Linux compat -- ideally it would be a userspace server.


It is a HAL implementation (kernel-hal/src/libos/), but the feature also
affects code outside the HAL: `zCore/src/main.rs` (no_std toggle), `fs.rs`
(HostFS vs SFS), `utils.rs` (std::env::args), `logging.rs` (chrono timestamps),
and `loader/` (run_fncall vs run). There are ~44 occurrences across 15 files.
About 30% could be eliminated with better abstraction boundaries, but the rest
are inherent to the two fundamentally different execution environments (std vs
no_std, real HW vs host OS).


`zCore/src/platform/` contains **pre-HAL bootstrap** code: assembly boot (page
tables, MMU enable), linker scripts (.ld), and entry points that must live in
the binary crate. kernel-hal's `bare/arch/aarch64/` handles the **post-boot
runtime** HAL (interrupts, timers, trap handling, memory mapping). The split
is: `platform/` = "get to Rust"; `kernel-hal/` = "run Rust OS services". Linker
scripts and binary entry points cannot be in library crates, which is why
platform/ exists in the binary crate.


You're right that ideally ALL platform-dependent code would be in the HAL. The
practical constraint is Rust's crate system: linker scripts (.ld) and the
`#[no_mangle] fn _start` entry point MUST be in the final binary crate.
Assembly files that set up initial page tables and stacks before any Rust code
can run also live here because they need `global_asm!` in the binary crate
context. However, you're right that `entry.rs` (the Rust code after assembly)
and `consts.rs` COULD be moved into kernel-hal with some refactoring.
See [#77](https://github.com/andrewdavidmackenzie/zCore/issues/77). Explore moving
all possible platform code into kernel-hal, leaving only the minimal binary-
crate-required bits (linker scripts, assembly stubs) in `zCore/src/platform/`.


The HAL uses `hal_fn_def!` / `hal_fn_impl!` macros instead of traits.
`hal_fn_def!` declares public free functions (e.g.,
`kernel_hal::thread::spawn`) that delegate to a hidden `__HalImpl` struct's
trait methods. `hal_fn_impl!` provides the concrete implementation for each
platform. Only one impl is compiled (bare or libos), so there's no runtime
dispatch -- it's monomorphized at compile time.

Advantages over traits: zero-cost (no vtable), clean call syntax
(`kernel_hal::foo()` vs `hal.foo()`), no generic parameter threading.
Disadvantages: hard to navigate in IDE, non- standard pattern.

Re "why are shared implementations in HAL": code in `bare/boot.rs`,
`bare/timer.rs`, `bare/net.rs` is shared across architectures but still HAL-
level (it uses hardware abstractions like `naive-timer`, `smoltcp`).
Architecture-specific code is in `bare/arch/{aarch64,riscv,x86_64}/`. The split
is defensible but could be cleaner with a more conventional trait-based
approach.
See [#78](https://github.com/andrewdavidmackenzie/zCore/issues/78).


**Structure:**
- `src/main.rs` -- `primary_main()` orchestration, `secondary_main()` for SMP
  cores
- `src/platform/aarch64/` -- AArch64 boot assembly, page tables, linker script
- `src/platform/riscv/` -- RISC-V boot (standard + C910 variants), Sv39 page
  tables
- `src/platform/x86/` -- x86_64 UEFI boot via rboot
- `src/platform/libos/` -- Library OS mode (runs as host userspace process)
- `src/handler.rs` -- Frame alloc/dealloc, page fault handling
- `src/memory.rs` / `src/memory_x86_64.rs` -- Architecture-specific allocators

Yes, both are used. `memory.rs` (aarch64/riscv64) provides a UNIFIED buddy
allocator (`customizable-buddy`) that serves as both `#[global_allocator]`
(heap for Vec, Box, etc.) AND the physical frame allocator. `memory_x86_64.rs`
uses TWO separate allocators: `buddy_system_allocator` for the heap and
`bitmap-allocator` for frame tracking. `region-alloc` (third-party) is used
only by `zircon-object` for PCI BAR address space management, not for general
memory allocation. The per-arch split exists because x86_64 gets memory info
from UEFI (bitmap-friendly) while aarch64/riscv64 discover memory at runtime
(buddy-friendly).


- `src/fs.rs` -- Rootfs/ZBI initialization

It's more than ramdisk. In Linux mode, `fs.rs` tries multiple sources in order:
(1) libos: HostFS passthrough to host filesystem, (2) linked-in image via
`.incbin` assembly, (3) init RAM disk from HAL, (4) real block device via
`kernel_hal::drivers:: all_block()` with BlockCache, opened as SFS. In Zircon
mode, it just returns raw ZBI bytes (no filesystem). So it supports real block
devices with caching when available.


- `*.json` -- Custom Rust bare-metal target specs per architecture
- `disk/` -- EFI boot disk for aarch64 UEFI

`zCore/disk/` exists on disk as a build artifact (gitignored). It contains
`EFI/Boot/bootaa64.efi` and `EFI/Boot/Boot.json`. It's created during the
aarch64 UEFI boot build path in the legacy `zCore/Makefile`. Currently the
xtask-based build uses a different boot method (`-kernel` flag to QEMU, not
UEFI), so this directory is less relevant for the current build flow.


Yes, `zCore/disk/` is only used by the legacy `zCore/Makefile` UEFI boot path.
It could be moved to `target/aarch64/release/disk/` or eliminated entirely
since the xtask-based build uses `-kernel` (direct kernel load) not UEFI.
See [#79](https://github.com/andrewdavidmackenzie/zCore/issues/79).


**Workspace dependencies:** `kernel-hal`, `loader`, `zircon-object`,
`executor`, `linux-object` (optional)

**Status:** Actively used. This is the primary build target (`cargo qemu --arch
aarch64`).

---

### `kernel-hal/` -- Hardware Abstraction Layer

**Purpose:** Provides a unified, architecture-independent interface for all
hardware interaction. Abstracts differences between three CPU architectures and
two execution modes (bare-metal vs libos).

**Key design:** Uses a macro-based trait dispatch system (`hal_fn_def!` /
`hal_fn_impl!`) that declares the full HAL interface and allows pluggable
implementations per platform.

See the detailed explanation under the zCore/ section above (the "Explain more
the HAL" TODO). In short: zero-cost (no vtable, monomorphized), clean free-
function call syntax, only one impl compiled per platform. Downsides: opaque to
IDE, non-standard. A trait approach would work but would require threading a
generic/dyn parameter
through every consumer. See [#78](https://github.com/andrewdavidmackenzie/zCore/issues/78).


Yes, a `trait Hal` with associated types could work: `kernel_hal::Hal<P:
Platform>` where each platform implements `Platform`. The kernel would be
generic over `P`. This is how some Rust OS projects do it (e.g., Hubris). The
cost: every type that touches the HAL needs a `P` parameter, which can be
verbose. The macro approach avoids this by using a single hidden impl struct.
Both are valid; traits would be more idiomatic Rust. Covered by the HAL cleanup
issue above.


**HAL interface modules:**
- `boot` -- init sequences, cmdline, initrd
- `cpu` -- cpu_id, frequency, reset
- `mem` -- phys_to_virt, physical memory read/write/copy
- `vm` -- page table activation, TLB flush

- TODO: Why does it make sense to separate memory and virtual memory, if the
  first one explicitly includes virtual memory functions.
  > `mem` = **physical memory** operations: enumerate
  > free regions, phys_to_virt/virt_to_phys address
  > conversion, direct read/write/copy of physical
  > memory, cache flush. `vm` = **virtual memory /
  > page table** operations: activate paging, flush
  > TLB, clone kernel page table entries. They operate
  > at different abstraction levels: physical memory
  > ops don't touch page tables; page table ops don't
  > directly read/write physical content. This is a
  > standard OS-level separation. The name `mem` could
  > be renamed to `pmem` for clarity.

- `interrupt` -- IRQ enable/disable/mask, MSI, IPI
- `thread` -- spawn, current thread tracking

Thread spawning differs fundamentally between platforms: bare-metal uses
`executor::spawn()` (custom bare-metal async executor with per-CPU run queues),
while libos uses `async_std::task::spawn()` (host OS thread pool). Also
`set/get_current_thread` uses per-CPU static arrays on bare-metal vs
`task_local!` on libos. This is inherently platform-specific ("which CPU am I
on?" has no portable answer), so it belongs in the HAL, not in the kernel.


- `timer` -- timer enable, now, set deadline
- `rand` -- fill_random
- `vdso` -- Zircon vDSO constants

vDSO (virtual Dynamic Shared Object) is a small shared library the kernel maps
into every process. It lets userspace call certain kernel functions (e.g., get
current time) WITHOUT a syscall, by reading kernel-maintained data directly.
zCore needs it because Zircon userspace binaries expect the vDSO for
`zx_ticks_get()` and `zx_clock_get_monotonic()`. The `VdsoConstants` struct
contains: max_num_cpus, cache line sizes, ticks_per_second, physmem,
version_string.


Yes, real Fuchsia/Zircon does exactly this. The vDSO is a core part of Zircon's
ABI, not a hack. It's mapped into every process by the kernel at a random
address. Functions like `zx_clock_get_monotonic()` read shared kernel data
structures directly (no syscall overhead). Linux does the same with `vdso.so`
for `gettimeofday()` and `clock_gettime()`. It's a well-established OS pattern
for high-frequency calls where syscall overhead matters.


- `console` -- early console output

**Structure:**
- `src/hal_fn.rs` -- Complete HAL interface declaration
- `src/common/` -- Shared types (addresses, contexts, futures, page tables,
  user pointers)
- `src/bare/` -- Bare-metal backend with `arch/aarch64`, `arch/riscv`,
  `arch/x86_64`

They exist at `kernel-hal/src/bare/arch/aarch64/`, `kernel-
hal/src/bare/arch/riscv/`, and `kernel-hal/src/bare/arch/x86_64/`. Each
contains: `mod.rs`, `config.rs`, `cpu.rs`, `drivers.rs`, `interrupt.rs`,
`mem.rs`, `timer.rs`, `trap.rs`, `vm.rs`. Confirmed present on disk.


- `src/libos/` -- LibOS backend (simulates hardware via mmap, tmpfiles, SDL)

Yes. `kernel-hal/Cargo.toml` defines `libos` as a feature: `libos = ["nix",
"tempfile", "async-std", "bitmap-allocator", "zcore-drivers/mock"]`. When libos
is off, `lib.rs` selects the `bare/` backend via `cfg_if!`; the entire `libos/`
module is not compiled. Note: `kernel-hal` defaults to `libos` being ON, but
`zCore` imports it with `default-features = false`.


Correct, libos runs without QEMU -- it's a regular host process. Currently
broken (see later
TODOs about build failures). See [#80](https://github.com/andrewdavidmackenzie/zCore/issues/80).


- `src/utils/` -- InitOnce, LazyInit, MpscQueue, generic PageTable

Not directly. `OnceLock` is `std::sync` only, not `no_std`. `InitOnce` wraps
`spin::Once` (already the no_std equivalent) and adds a default-value fallback.
`LazyInit` provides `DerefMut` and in-place init that neither `OnceLock` nor
`spin::Once` offer. However, `lazy_static` with `spin_no_std` COULD be replaced
by `spin::Lazy` (from the `spin` crate, already a dependency), eliminating the
`lazy_static` dep.
See [#81](https://github.com/andrewdavidmackenzie/zCore/issues/81).


- `src/kernel_handler.rs` -- `KernelHandler` trait (callbacks from HAL into
  kernel)

`kernel-hal` is a library crate; `zCore` is the binary crate. The dependency
flows one way: `zCore -> kernel-hal`. The HAL cannot `use zCore` (circular
dependency). But the HAL needs to allocate physical frames (managed by zCore's
allocator). Solution: **dependency inversion**. The HAL defines `KernelHandler`
trait; zCore implements it (`ZcoreKernelHandler`); zCore passes `&'static
ZcoreKernelHandler` during init. HAL stores it in a global and calls through
the trait. This is a standard pattern in layered systems.


The callback pattern exists solely because Rust prevents circular crate
dependencies. kernel-hal (library) cannot depend on zCore (binary). The
allocator lives in zCore because it needs the `#[global_allocator]` attribute
(binary crate only). Simplification options: (1) Move the allocator into
kernel-hal itself (requires making kernel-hal the binary crate or using a
separate allocator crate). (2) Create a `kernel-alloc` crate that both kernel-
hal and zCore depend on. (3) Make the page fault handler a function pointer
registered at init, not a trait. All would eliminate KernelHandler. Covered by
HAL cleanup issue.


**Workspace dependencies:** `drivers` (zcore-drivers), `executor` (bare-metal
only)

**Status:** Actively used. Core infrastructure crate.

TODO Describe this more, including the "scheme" concept
  > The "Scheme" concept is in the `drivers` crate,
  > not kernel-hal. See the `drivers/` section below.
  > `Scheme` is a base trait all drivers implement
  > (provides `name()` and `handle_irq()`). Specific
  > device traits extend it: `BlockScheme` (read/write
  > blocks), `UartScheme` (send/recv bytes),
  > `NetScheme` (send/recv packets), `DisplayScheme`
  > (framebuffer), `InputScheme` (events),
  > `IrqScheme` (interrupt controller). kernel-hal
  > re-exports these traits and manages device
  > registries (`DeviceList<T>`) with accessors like
  > `all_block()`, `all_uart()`, etc.

---

### `zCore/zircon-object/` -- Zircon Kernel Object Library

**Purpose:** Implements all Zircon kernel objects -- the fundamental
abstractions of Google's Fuchsia/Zircon microkernel. This is the core of zCore,
used by both Zircon and Linux personalities.

Cargo supports having a `[lib]` and `[[bin]]` in the same
crate. But zircon-object is also depended on by `linux-object`, `linux-
syscall`, `zircon-syscall`, and `loader`. Making it a sub-package of zCore
would create a circular dependency (those crates can't depend on zCore).
Keeping it as a separate library crate is the correct architecture.


"Zircon personality" means zCore boots the Fuchsia **userboot** process -- the
first userspace program in the Fuchsia boot sequence. It reimplements the
Zircon KERNEL side (kernel objects + syscalls) but runs REAL Fuchsia prebuilt
userspace binaries (userboot.so, libzircon.so). It's not a full Fuchsia
personality (no component framework, no FIDL, no package management) -- just
enough to boot userboot and run Zircon core tests. A full Fuchsia stack would
require many more userspace services.


Google's Zircon kernel (written in C++) provides ~25 kernel object types
(Process, Thread, Channel, VMO, VMAR, Port, etc.) and ~167 syscalls. zCore
reimplements these IN RUST: `zircon-object` has Rust versions of all those
object types, and `zircon-syscall` reimplements the syscall handlers. The
userspace side (userboot.so, libzircon.so) is NOT reimplemented -- real
Fuchsia-compiled binaries are used. So zCore replaces only the kernel half,
keeping the real userspace half.


**Kernel objects implemented (25 types):**
- **Task:** Job, Process, Thread, SuspendToken
- **IPC:** Channel, Socket, Fifo
- **Signaling:** Event, EventPair, Futex, Port, Timer
- **Virtual Memory:** VmObject (Paged/Physical/Slice),
  VmAddressRegion (VMAR), Stream
- **Device:** Resource, Iommu, BTI, PMT, Interrupt,
  PcieDeviceKObject
- **Hypervisor** (feature-gated): Guest, Vcpu
- **Other:** DebugLog, ExceptionObject

**Yes, an MMU is required.** `VmAddressRegion` holds an `Arc<Mutex<dyn
GenericPageTable>>` and all mapping operations go through page tables. There is
no MMU-less fallback. Zircon's security model (handle-based capabilities,
process isolation) fundamentally depends on virtual memory. Running on a CPU
without an MMU (e.g., Cortex-M) would require a different kernel architecture.


Exactly right. The page table is a shared mutable data structure: multiple
threads (on multiple cores) can map/unmap pages in the same address space
concurrently (e.g., mmap from different threads, page fault handling, fork
COW). The Mutex serializes access to prevent data races on the page table
entries. In practice, contention is low because different threads usually touch
different VMARs. Real Zircon uses similar locking on its aspace (address space)
objects.


The `hypervisor` feature enables `Guest` and `Vcpu` kernel objects for running
virtual machines, plus 7 Zircon syscalls (guest_create, vcpu_create,
vcpu_resume, etc.). It depends on the `rvm` crate (RISC-V Virtual Machine).
However, the `rvm` dependency is **commented out** in both `zircon-
object/Cargo.toml` and `zCore/zircon-syscall/Cargo.toml`. The feature flag is defined
as empty (`hypervisor = []`). The code exists but **will not compile** without
uncommenting and updating the rvm dependency. **The feature is currently non-
functional.**


Yes, there is real code: `zCore/zircon-object/src/hypervisor/guest.rs` and `vcpu.rs`
define `Guest` and `Vcpu` structs with methods. `zCore/zircon-syscall/src/hypervisor.rs`
has 7 syscall handlers. But this code cannot compile because
the `rvm` crate dependency is commented out. Removing the feature flag and code
would lose ~200 lines but they're currently dead code. Could be preserved
behind the feature flag as-is until rvm is restored.


**Key patterns:**
- `KObjectBase` embedded in every object (provides KoID, name, signals)
- `impl_kobject!` macro auto-implements `KernelObject` trait

`impl_kobject!` generates a full `impl KernelObject for $class` that delegates
to `self.base` (a `KObjectBase` field). A derive proc-macro could do this but
requires a separate compilation unit. A blanket trait impl is impossible
because Rust has no "has field X" trait bound. The declarative macro is the
simplest approach: it assumes the struct has `base: KObjectBase` and generates
id(), name(), signal(), etc. delegations. It also accepts optional method
overrides for `get_child()`, `peer()`, etc.


- Handle-based capability system with `Rights` bitflags
- Interior mutability via `lock::Mutex` throughout
- Async/await for blocking operations (wait_signal, Port::wait, Futex::wait)

**Workspace dependencies:** `kernel-hal`, `region-alloc`

**Status:** Actively used. Foundation crate for the entire kernel.

---

### `linux-object/` -- Linux Kernel Object Layer

**Purpose:** Implements Linux-compatible abstractions on top of the Zircon
object model. Enables running Linux userspace programs by emulating Linux
kernel data structures.

**Both, but heavily weighted toward concrete implementations.** Traits
(`ProcessExt`, `ThreadExt`, `FileLike`) are defined as extension points, but
the bulk is 885+ lines of `LinuxProcess` implementation, full filesystem layers
(DevFS, MountFS, RamFS), ELF loading, SysV IPC, signal handling, and networking
-- all concrete implementations.


**Key subsystems:**
- **Process/Thread:** `LinuxProcess` / `LinuxThread` as
  extension traits on Zircon Process/Thread
- **Filesystem:** `FileLike` trait, File, Pipe, EpollFile,
EventFd, Stdin/Stdout, devfs (/dev/null, /dev/random, /dev/fb0, /dev/ttyS*,
/dev/input/*)
- **Signals:** Full Linux signal model (1-64),
SignalAction, signal delivery with arch-specific MachineContext
- **Networking:** TCP, UDP, Raw, Netlink sockets via
  smoltcp
- **IPC:** System V semaphores and shared memory
- **Sync:** EventBus, counting Semaphore
- **ELF Loader:** `LinuxElfLoader` with dynamic linker
  support, auxv setup

No -- in real Linux, the kernel loads ELF binaries too (`fs/binfmt_elf.c`). The
kernel parses ELF headers, maps segments into the process address space, loads
the dynamic linker (PT_INTERP), builds the auxiliary vector (AT_PHDR,
AT_RANDOM, etc.), and sets up the user stack with argc/argv/envp/auxv.
Userspace `ld.so` handles shared library loading after that, but the initial
ELF binary + interpreter are kernel work. The `LinuxElfLoader` here does
exactly this.


Yes, the kernel must do it because: (1) the process doesn't exist yet --
there's no userspace to run code in until the ELF is loaded, (2) mapping memory
segments requires manipulating the VMAR (kernel-side page table management),
(3) the auxiliary vector includes kernel-internal data (page size, vDSO
address, random bytes). In Fuchsia/Zircon, this is done differently:
`process_builder` in userspace creates processes using Zircon syscalls
(vmo_create, vmar_map), but that requires an already-running process to do the
loading. The FIRST process (userboot) is loaded by the kernel. Same in Linux:
the kernel loads the initial process; subsequent processes are loaded by the
kernel via execve().


**Workspace dependencies:** `zircon-object`, `kernel-hal`, `drivers` (zcore-
drivers)

**Status:** Actively used. Required for Linux mode.

---

### `linux-syscall/` -- Linux Syscall Dispatch

**Purpose:** Receives raw Linux syscall numbers and arguments from userspace,
dispatches them to handler functions, and returns results. Implements ~130
syscalls.

**Syscall categories:** File I/O (~25), FD management (~10), directories (~12),
stat (~4), I/O multiplexing (~8), process/thread (~12), signals (~8), virtual
memory (~5), time (~15), IPC (~7), networking (~16), system/misc (~20+)

**Key design:**
- `build.rs` generates `SyscallType` enum from `.h.in` header files (one per
  arch)
- All blocking syscalls are async
- Architecture-specific code paths for x86_64, aarch64, riscv64 (different
  syscall numbers and struct layouts)
- `test/` directory has C test programs for manual validation

They ARE used: `xtask/src/linux/test.rs` cross- compiles them and copies
binaries into rootfs via the `other-test` subcommand. They are NOT in the main
CI `test` target (which runs boot-test + libc-test), but available via `cargo
other-test`. They do NOT overlap with libc-test: these test zCore-specific IPC
(pipes, SysV semaphores, shared memory, poll, select) while libc-test focuses
on libc conformance (math, string, stdio). They are complementary.


Yes, they should. They test important IPC and syscall functionality not covered
by libc-test.
See [#82](https://github.com/andrewdavidmackenzie/zCore/issues/82).


**Workspace dependencies:** `zircon-object`, `linux-object`, `kernel-hal`

Because zircon-object provides the SHARED kernel object model used by both
personalities. linux-syscall directly uses: `Process`, `Thread`,
`CurrentThread`, `ThreadFn` (task management), `VmObject`, `MMUFlags`,
`PAGE_SIZE` (VM), `KernelObject`, `Signal` (signaling), `ThreadState` (futex
blocking). These are the core types -- linux-object extends them with Linux-
specific behavior but doesn't re-export everything. It would be cleaner if
linux-object re-exported all needed zircon types, but currently linux-syscall
reaches through to both layers.


**Status:** Actively used. Required for Linux mode.

---

### `zCore/zircon-syscall/` -- Zircon Syscall Dispatch

Note: `zircon-syscall` is a separate library crate (not merged into the `zcore`
binary crate) because `linux-object`, `linux-syscall`, and `loader` all depend
on it. Making it a sub-package of the `zcore` binary would create a circular
dependency. It is co-located under `zCore/` for organizational clarity.


**Purpose:** Implements the Fuchsia Zircon kernel ABI -- translates raw syscall
numbers into calls on Zircon kernel objects. Implements ~100+ of the 167
defined syscalls.

Unimplemented syscalls by category
(see [#83](https://github.com/andrewdavidmackenzie/zCore/issues/83)):

- **Pager (5)**: pager_create, pager_create_vmo,
  pager_detach_vmo, pager_supply_pages,
  pager_op_range -- requires a full user pager
  subsystem. MAJOR effort.
- **System (3)**: system_mexec, _payload_get,
  system_powerctl -- kexec and power management.
  MEDIUM effort.
- **Tracing/Debug (5)**: ktrace_read/control/write,
  mtrace_control, debug_send_command -- kernel
  tracing infrastructure. MEDIUM effort.
- **Framebuffer (2)**: framebuffer_get_info,
  framebuffer_set_range -- legacy, deprecated in
  Fuchsia. LOW priority.
- **Futex (2)**: futex_requeue_single_owner,
  futex_get_owner -- extensions to existing futex.
  SMALL effort.
- **MSI (2)**: msi_allocate, msi_create -- MSI
  interrupt allocation. MEDIUM effort.
- **Misc (5)**: profile_create, smc_call,
  ioports_release, object_set_profile,
  vmar_op_range -- varied. SMALL-MEDIUM.
- **Clock (3)**: clock detail/via_kernel variants.
  SMALL effort.
- **Test (12)**: syscall_test_0 through _8 + wrapper
  + handle_create -- only for Fuchsia test infra.
  SMALL effort but low value.
- **Hypervisor (8)**: guest/vcpu ops -- blocked on
  rvm crate restoration. MAJOR effort.

Tracking Zircon syscall interface changes:
see [#84](https://github.com/andrewdavidmackenzie/zCore/issues/84).

Long-term goal: running full Fuchsia on zCore. This
requires: (1) all ~167 Zircon syscalls, (2) the pager
subsystem, (3) exception channels working end-to-end,
(4) component framework support in userspace. See
[#85](https://github.com/andrewdavidmackenzie/zCore/issues/85).

**Syscall categories:** Handles (4), objects (8), tasks (17), channels (7),
sockets (4), FIFOs (3), VMOs (11), VMARs (5), streams (6), signals (5), ports
(3), futex (4), time (6), DDK/device (11), PCI (10), debug (5), exceptions (3),
resource (1), system (1), CPRNG (1), composite exit (2), hypervisor (7,
feature-gated)

**Key design:**
- `build.rs` generates `SyscallType` from Fuchsia's official `zx-syscall-
  numbers.h`
- Rights enforcement on every handle access
- User memory access via typed safe pointers
- Async for blocking syscalls

**Workspace dependencies:** `zircon-object`, `kernel-hal`

Confirmed: `zCore/zircon-syscall/Cargo.toml` depends on `zircon-object` and `kernel-
hal` only. No dependency on the `zCore` binary crate. The dependency flows one
way: `zCore` -> `loader` -> `zircon-syscall`. Syscall numbers come from `zx-
syscall-numbers.h` (compiled at build time via `build.rs`), not from any
runtime link.


**Status:** Actively used for Zircon mode. Less active than Linux mode
currently.

**Currently cannot boot Zircon mode.** It requires prebuilt binaries
(userboot.so, libzircon.so, bringup.zbi) at `prebuilt/zircon/{arch}/` which are
NOT present in the repo. To generate them: run `tools/scripts/gen-prebuilt.sh` inside
a Fuchsia source tree. The Zircon integration test (`loader/tests/zircon.rs`)
is x86_64-only and expects `prebuilt/zircon/x64/bringup.zbi`.
See [#86](https://github.com/andrewdavidmackenzie/zCore/issues/86).


---

### `loader/` -- User Program Loader

**Purpose:** The integration hub that loads ELF binaries and runs the
user/kernel trap loop. Supports both Linux and Zircon modes.

**Two modes:**
- **Linux** (`linux` feature): Loads Linux ELF, runs async
  trap loop dispatching Linux syscalls, handles signals
- **Zircon** (`zircon` feature): Implements Fuchsia
`userboot` protocol, loads prebuilt userboot/vDSO, sets up initial channel with
15 handles

**Structure:**
- `src/linux.rs` -- Linux process lifecycle, trap loop, signal delivery
- `src/zircon.rs` -- Zircon userboot protocol, exception-based trap loop
- `examples/` -- Runnable examples for libos mode

**`linux-libos.rs`**: Runs a Linux ELF in libos mode. Creates HostFS from
`rootfs/libos/`, calls `zcore_loader::linux::run()`, waits for exit. Run:
`cargo run -p zcore-loader --example linux-libos --features "linux,libos" --
/bin/busybox` **`zircon-libos.rs`**: Runs Zircon userboot in libos mode. Reads
a ZBI file, calls `zcore_loader::zircon::run_userboot()`, waits for
USER_SIGNAL_0. Run: `cargo run -p zcore-loader --example zircon-libos
--features "zircon,libos" -- path/to/bringup.zbi` Both run as normal host
processes (no QEMU).


- `tests/` -- Integration tests (busybox commands, Zircon userboot)

A ZBI (Zircon Boot Image) is Fuchsia's boot format: a concatenation of typed
items including the kernel, bootfs (initial filesystem), command line, and
platform data. The prebuilt `bringup.zbi` contains the bootfs with `userboot`
(Fuchsia's first process) and essential libraries. `tests/zircon.rs` is 8
lines: reads the ZBI file, calls `run_userboot(zbi, cmdline)`, waits for
`USER_SIGNAL_0` indicating userboot completed. It's x86_64 only and requires
the prebuilt files.


`loader/tests/linux.rs` (161 lines): Uses `#[async_std::test]` to run busybox
commands in libos mode via HostFS from `rootfs/libos/`. Tests: `test_busybox`,
`test_uname`, `test_date_time`, `test_dir`, `test_create_remove_file`, etc.
Covers basic commands, file ops, and syscall unit tests (testpipe1, testsem1,
testshm1, testpoll). `loader/tests/zircon.rs` (8 lines): Single test loading
bringup.zbi, x86_64 only. These are `cargo test` integration tests, NOT in the
CI `make test` target (which runs QEMU-based boot- test + libc-test). Run with:
`cargo test -p zcore-loader`


`make test` runs boot-test + libc-test (both QEMU-based), NOT these libos
integration tests. CI `test.yml` runs `cargo test --no-fail-fast` which builds
the default workspace member (xtask), so it does NOT run zcore-loader tests
either. These tests are only run manually via `cargo test -p zcore-loader
--features linux,libos`.
See [#80](https://github.com/andrewdavidmackenzie/zCore/issues/80).


**Workspace dependencies:** `kernel-hal`, `zircon-object`, `linux-object`,
`linux-syscall`, `zircon-syscall`, `executor`

**Status:** Actively used. Central integration point.

---

### `drivers/` -- Device Driver Framework (`zcore-drivers`)

It's both. The `scheme/` module defines the framework: trait interfaces
(`BlockScheme`, `UartScheme`, `NetScheme`, etc.) and the `Device` enum. The
rest (uart/, irq/, virtio/, net/, display/, input/, nvme/) contains concrete
driver implementations. The `DevicetreeDriverBuilder` provides auto-discovery
from device trees. A new driver would implement the relevant Scheme trait and
register via the builder. So it's a framework WITH a set of bundled drivers.


**Purpose:** Unified device driver framework with trait-based interfaces and
concrete implementations for multiple architectures.

The `drivers` crate has NO workspace dependencies and communicates via extern
"C" FFI. This decouples drivers from the kernel object model, making them
potentially reusable in other OS projects. kernel-hal's `drivers.rs` acts as a
consumer: it re-exports scheme traits, manages device registries, and provides
the FFI functions (virtio_dma_alloc, drivers_phys_to_virt) that drivers call
for DMA and address translation. The trait definitions are in `drivers/`
because the driver implementations need them, and putting them in kernel-hal
would create a circular dependency (kernel-hal already depends on drivers).


**Driver types:**
- **Interrupt controllers:** GIC-400 (aarch64), PLIC
  (riscv64), APIC (x86_64)
- **UARTs:** PL011, 16550 (MMIO + PMIO), Allwinner, FU740
- **Block:** VirtIO block, NVMe
- **Display:** VirtIO GPU, UEFI framebuffer
- **Network:** E1000, Realtek, loopback
- **Input:** VirtIO input
- **Mock:** SDL-based mock drivers for libos mode

For QEMU virt-aarch64, the following are initialized (`kernel-
hal/src/bare/arch/aarch64/ drivers.rs`):
- **PL011 UART** at 0x0900_0000 (wrapped in
  BufferedUart)
- **GIC-400** (GICv2) at 0x0800_0000 with IRQ 30
  (timer) and IRQ 33 (UART)
- **VirtIO block** at 0x0a00_0000
For Raspberry Pi: RPi uses a BCM283x SoC with a mini-UART and VideoCore GPU --
different from QEMU virt. The PL011 driver would work (RPi has a PL011), but
GIC and VirtIO would not (RPi uses a BCM interrupt controller). RPi support
would
need new drivers. See [#87](https://github.com/andrewdavidmackenzie/zCore/issues/87).


Yes. QEMU virt-aarch64 provides a **PL011 UART** at physical address
**0x0900_0000** (region size 0x1000). IRQ 33 through the GIC. The PL011 driver
at `drivers/src/uart/uart_pl011.rs` handles it. The base address is configured
in `zCore/src/platform/aarch64/entry.rs:9` as `uart_base: 0x0900_0000` in
`KernelConfig`.


**Key design:**
- `Scheme` traits define driver interfaces (BlockScheme, UartScheme, NetScheme,
  etc.)
- `DevicetreeDriverBuilder` walks FDT for auto-discovery (aarch64/riscv64)
- Communicates with kernel via extern "C" FFI (no workspace crate dependencies)

**Workspace dependencies:** None (self-contained, uses FFI to kernel)

**No, drivers run in kernel (supervisor) mode.** The `drivers` crate is a
library linked into the final `zCore` kernel ELF via kernel-hal. The FFI
boundary (`extern "C"`) is a link-time abstraction for crate decoupling, not a
process boundary. All driver code executes in supervisor mode alongside the
rest of the kernel.


**Status:** Actively used. Essential for hardware interaction.

---

### `xtask/` -- Build Task Runner

**Purpose:** Implements the cargo-xtask pattern, providing a type-safe CLI for
all build, test, rootfs, and QEMU operations.

**Key subcommands:** `qemu`, `bin`, `image`, `rootfs`, `check-style`, `zircon-
init`, `gdb`, `asm`, `libc-test`, and more.

| Command          | Purpose                  |
|------------------|--------------------------|
| `git-proxy`      | Set/unset git proxy      |
| `dump`           | Print build/VCS metadata |
| `zircon-init`    | Download Zircon prebuilts|
| `update-all`     | Update submodules +      |
|                  | toolchain + deps         |
| `check-style`    | fmt + clippy + doc check |
| `asm`            | Dump kernel disassembly  |
| `bin`            | Strip kernel to raw bin  |
| `qemu`           | Build + run in QEMU      |
| `gdb`            | Launch GDB to QEMU       |
| `rootfs`         | Build Linux rootfs       |
| `musl-libs`      | Copy musl .so to rootfs  |
| `ffmpeg`         | Cross-compile FFmpeg     |
| `opencv`         | Cross-compile OpenCV     |
| `libc-test`      | Copy libc-test to rootfs |
| `other-test`     | Copy misc tests to rootfs|
| `image`          | Pack rootfs into SFS img |
| `libos-libc-test`| Build libos rootfs+tests |
| `linux-libos`    | Run zCore in libos mode  |


**Structure:**
- `src/main.rs` -- CLI definition (clap derive), 21 subcommands
- `src/build.rs` -- Core build orchestration, QEMU launch
- `src/arch.rs` -- Architecture enum, musl cross-toolchain resolution
- `src/linux/` -- Rootfs building, image creation, test deployment

**Workspace dependencies:** `z-config`

**Status:** Actively used. Default build entry point.

---

### `z-config/` -- Machine Configuration

**Purpose:** Parses `config/machine-features.toml` and provides `MachineConfig`
structs to xtask for selecting architecture, features, and build options per
machine target.

**Defined machines:** `virt-aarch64`, `virt-riscv64`, `virt-x86_64`, `nezha`
(Allwinner D1), `cr1825` (T-HEAD), `visionfive` (StarFive), `fu740` (SiFive)

**Workspace dependencies:** None (uses serde + toml)

**Status:** Actively used. Required by xtask.

It's only 55 lines and only used by xtask. It could be inlined. The reason it's
separate: as a crate it uses `CARGO_MANIFEST_DIR` for path resolution to
`config/machine-features.toml`, which would break if moved. If only xtask uses
it, inlining with adjusted paths would work fine. Marginal benefit though.


---

### `third-party/executor/` -- Async Task Executor

**Purpose:** A `no_std` async task executor for bare-metal environments.
Provides `spawn()`, `run_until_idle()`, and `sched_yield()`.

**Key detail:** Forked/vendored with local modifications (removed trapframe
dependency, architecture-specific scheduling).

**Workspace dependencies:** None

**Status:** Actively used. Depended on by `kernel-hal`, `loader`, and `zCore`.

The executor is the kernel's scheduler on bare metal. Each CPU runs
`run_until_idle()` in an infinite loop. It picks tasks (futures) from a
`TaskCollection`, polls them, and context-switches between a "strong executor"
(main) and "weak executors" (for interrupted futures). If a future is
interrupted by a timer, the strong executor is demoted and a new one created,
preventing starvation. Work stealing across CPUs is supported. When no tasks
are ready, it calls `wait_for_interrupt()` (WFI/HLT). In libos mode, this is
replaced by `async_std` -- the executor crate is only used on bare metal.


Traditional kernels (Linux, Zircon) use preemptive scheduling: timer interrupts
force context switches via saved register state. zCore uses cooperative async:
each kernel "thread" is a Future that yields at `.await` points. The executor
polls futures and switches between them. **Benefits**: (1) no need for per-
thread kernel stacks (futures are state machines on the heap), (2) simpler
synchronization (no preemption between await points), (3) natural fit for async
I/O (VirtIO, network), (4) same code runs on bare metal and in libos mode.
**Downsides**: (1) a CPU-bound future that doesn't yield can starve others
(mitigated by the executor's strong/weak promotion system), (2) less
predictable latency than a preemptive scheduler, (3) the pattern is unusual for
OS kernels and harder to reason about for developers used to traditional
kernels.


---

### `third-party/region-alloc/` -- Region Allocator

**Purpose:** A `no_std` BTreeSet-based memory region allocator supporting add,
subtract, allocate-by-address, and allocate-by-size with alignment.

**Workspace dependencies:** None

**Status:** Actively used. Depended on by `zircon-object` (for VMAR address
allocation).

`region-alloc` is used ONLY for PCI BAR (Base Address Register) allocation in
`zCore/zircon-object/src/dev/pci/bus.rs` and `nodes.rs`. Methods actually called:
- `add(base, size)` -- add an address region
- `add_or_subtract(base, size, is_add)` -- add or remove regions
- `allocate_by_addr(base, size)` -- allocate a specific address range
- `allocate_by_size(size, alignment)` -- allocate by size with alignment Three
  instances manage MMIO-low (32-bit), MMIO-high (64-bit), and PIO (port I/O)
  spaces.


The PCI code in zircon-object is always compiled (not feature-gated). However,
it's only exercised at runtime when the machine has PCI support (controlled by
`pci_support` in `machine-features.toml`). QEMU virt machines have PCI;
embedded boards (nezha, cr1825, visionfive) do not. The `no-pci` feature in the
drivers crate skips PCI bus scanning. The region-alloc code is dormant on non-
PCI machines.

---

## Excluded Standalone Projects

These are in the repository but explicitly excluded from the workspace via
`exclude = ["petal", "rboot"]`.

### `petal/` -- Minimal Zircon Test Userspace

**Purpose:** A minimal, controlled userspace for testing the zCore Zircon
kernel. Named after a small part of the Fuchsia flower, petal is an alternative
to the full Fuchsia userspace stack -- simple test programs that exercise Zircon
syscalls directly.

**Contents:** A single `hello.rs` binary. Has its own `Cargo.toml`,
`Cargo.lock`, and `rust-toolchain.toml`. Currently not functional (requires
userstart and vDSO from #121).

**Targets:** `x86_64-unknown-fuchsia` and `aarch64-unknown-fuchsia` (Rust
tier-3 targets).

Next steps for petal
(see [#89](https://github.com/andrewdavidmackenzie/zCore/issues/89)):
(1) add Zircon syscall bindings, (2) create test
programs, (3) build and package into a ZBI, (4) add
CI test that boots in Zircon mode. Requires userstart
and vDSO from #121 first.

---

### `rboot/` -- x86_64 UEFI Bootloader (Git Submodule)

**Purpose:** UEFI bootloader for x86_64. Loads a kernel ELF and boots it.
Points to `https://github.com/rcore-os/rboot.git`.

**Status:** Partial / Legacy. Only relevant for x86_64 UEFI boot, which is not
the current primary development target. Developer docs note it "is a legacy
issue, will be resolved in the future."

---

## Git Submodules

### `libc-test/` -- musl libc Test Suite (Git Submodule)

**Purpose:** The musl libc conformance test suite (C project). Used to validate
zCore's Linux syscall compatibility.

**Points to:** `https://github.com/rcore-os/libc-test`

**Status:** Actively used in CI. The `test.yml` workflow runs libc functional
tests in QEMU via `tools/scripts/libc-test.sh`.

---

## Non-Crate Folders
