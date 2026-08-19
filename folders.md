# zCore Project Folder Analysis

## Overview

zCore is a Rust reimplementation of the Zircon microkernel
(from Google's Fuchsia OS) that also supports running Linux
userspace programs. It targets three CPU architectures
(aarch64, riscv64, x86_64) and can run both bare-metal and
as a "library OS" on a host system.

The project is organized as a Cargo workspace with 12 member
crates plus 2 excluded standalone projects and 2 git
submodules.

---

## Workspace Member Crates

These are the Rust crates listed in the root `Cargo.toml`
workspace `members` array.

### `zCore/` -- Main Kernel Binary

**Purpose:** The top-level kernel binary that ties everything
together. This is the entry point for kernel execution -- it
boots the hardware, initializes memory, sets up the HAL, and
launches either a Linux or Zircon userspace.

TODO: Doesn't the bootloader boot the hardware?

TODO: Tell me more about the Zircon userspace. Where is the code, 
and what does it include.

**Key responsibilities:**
- Platform-specific boot assembly and entry points
  (per-arch `platform/` subdirs)

TODO: Could boot.s be replaced by rust code also?

- Global heap allocator and physical frame allocator
- `KernelHandler` implementation bridging the HAL to the
  memory allocator

TODO: Isn't "KernelHandler" a bad name? 

- OS personality selection (Linux vs Zircon, mutually
  exclusive via features)

TODO: Tell me which code this is and what features are
used to switch between the two.

TODO: Why does the kernel need to know about the OS 
personality that is to be run on top o fit?

TODO: Analyze could we have a cleaner kernel with no features 
and then the build system decides to run one (or both?) 
personalities on top of it?

- Filesystem initialization and root process launch

TODO: Tell me more about Zircon and file systems. Does it
need to be part of the microkernel, or can the filesystem
run in user space. WHat FS does Zircon support, what FS are 
included here, and what other FS does Fuschia support on
top of Zircon?

TODO: Why does libos feature appear many times, shouldn't
that just be another HAL implementation?

TODO: WHy is platform dependent code under zCore, shouldn't 
it be that each platform and board implement the HAL?

TODO: Explain more the HA and how it is defined. I expected
the HAL to be basically a set of traits that different 
platform and board combinations would implement - but 
it is a lot of macros plus implementation functions, many
empty. For the more complex real implementations in HAL, if
they are always the same why are they not part of the kernel
and not the HAL?

**Structure:**
- `src/main.rs` -- `primary_main()` orchestration,
  `secondary_main()` for SMP cores
- `src/platform/aarch64/` -- AArch64 boot assembly, page
  tables, linker script
- `src/platform/riscv/` -- RISC-V boot (standard + C910
  variants), Sv39 page tables
- `src/platform/x86/` -- x86_64 UEFI boot via rboot
- `src/platform/libos/` -- Library OS mode (runs as host
  userspace process)
- `src/handler.rs` -- Frame alloc/dealloc, page fault
  handling
- `src/memory.rs` / `src/memory_x86_64.rs` --
  Architecture-specific allocators

TODO: are these architecture allocators used, as we 
also have a separate crate that is an allocator. If both
are used explain why and for what.

- `src/fs.rs` -- Rootfs/ZBI initialization

TODO: Explain more, seems to be just ram disk, not general FS.

- `*.json` -- Custom Rust bare-metal target specs per
  architecture
- `disk/` -- EFI boot disk for aarch64 UEFI

TODO: Where is this folder

**Workspace dependencies:** `kernel-hal`, `loader`,
`zircon-object`, `executor`, `linux-object` (optional)

**Status:** Actively used. This is the primary build target
(`cargo qemu --arch aarch64`).

---

### `kernel-hal/` -- Hardware Abstraction Layer

**Purpose:** Provides a unified, architecture-independent
interface for all hardware interaction. Abstracts differences
between three CPU architectures and two execution modes
(bare-metal vs libos).

**Key design:** Uses a macro-based trait dispatch system
(`hal_fn_def!` / `hal_fn_impl!`) that declares the full HAL
interface and allows pluggable implementations per platform.

TODO: Explain why that is better than a normal rust trait and
implementation approach?

**HAL interface modules:**
- `boot` -- init sequences, cmdline, initrd
- `cpu` -- cpu_id, frequency, reset
- `mem` -- phys_to_virt, physical memory read/write/copy
- `vm` -- page table activation, TLB flush
TODO: Why does it make sense to separate memory and virtual
- memory, if the first one explicitly includes virtual memory
- functions.
- 
- `interrupt` -- IRQ enable/disable/mask, MSI, IPI
- `thread` -- spawn, current thread tracking

TODO: Why is this in the HAL and not in the kernel?
Why not in the platform dependent perts of the kernel?

- `timer` -- timer enable, now, set deadline
- `rand` -- fill_random
- `vdso` -- Zircon vDSO constants

TODO: Explain more about vDSO

- `console` -- early console output

**Structure:**
- `src/hal_fn.rs` -- Complete HAL interface declaration
- `src/common/` -- Shared types (addresses, contexts,
  futures, page tables, user pointers)
- `src/bare/` -- Bare-metal backend with
  `arch/aarch64`, `arch/riscv`, `arch/x86_64`

TODO: I don't see these folders in kernel-hal.

- `src/libos/` -- LibOS backend (simulates hardware via
  mmap, tmpfiles, SDL)

TODO: Is libos building controlled by a feature? If so
is building these entire modules skipped when the feature is 
not used?

TODO: Explore if libos builds and runs (without needing qemu right?)
and if so, incorporate it into our standard Makefile build and test
and CI as well. Include libos in a "all features" test that chacks for
building combinations of features.

- `src/utils/` -- InitOnce, LazyInit, MpscQueue, generic
  PageTable

TODO: Explore replacing InitOnce and LazyInit with new standard rust 
features, if they are available on std.

- `src/kernel_handler.rs` -- `KernelHandler` trait
  (callbacks from HAL into kernel)

TODO: Explain this more. What does it mean that the HAL calls back into
the kernel, isn't HAL code built as part of the kernel.

**Workspace dependencies:** `drivers` (zcore-drivers),
`executor` (bare-metal only)

**Status:** Actively used. Core infrastructure crate.

TODO Describe this more, including the "scheme" concept

---

### `zircon-object/` -- Zircon Kernel Object Library

**Purpose:** Implements all Zircon kernel objects -- the
fundamental abstractions of Google's Fuchsia/Zircon
microkernel. This is the core of zCore, used by both Zircon
and Linux personalities.

TODO: Tell me more about the Zircon personality and what it
implements. Is that user space parts of zircon? Is it really
Fuschia personality?

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

TODO: Clarify if Virtual memory, and an MMU is a must for Zircon, or
it can be built with only physical memory for simpler boards and CPUs?

TODO: Tell me more about Hypervisor. I have also seen a "hypervisor" feature 
elsewhere.

**Key patterns:**
- `KObjectBase` embedded in every object (provides KoID,
  name, signals)
- `impl_kobject!` macro auto-implements `KernelObject`
  trait

TODO: Tell us why the macro approach, and not just standard rust
trit implementation.

- Handle-based capability system with `Rights` bitflags
- Interior mutability via `lock::Mutex` throughout
- Async/await for blocking operations (wait_signal,
  Port::wait, Futex::wait)

**Workspace dependencies:** `kernel-hal`, `region-alloc`

**Status:** Actively used. Foundation crate for the entire
kernel.

---

### `linux-object/` -- Linux Kernel Object Layer

**Purpose:** Implements Linux-compatible abstractions on top
of the Zircon object model. Enables running Linux userspace
programs by emulating Linux kernel data structures.

TODO: Clarify if these are just trait definitions for a
linux layer, or implementation also.

**Key subsystems:**
- **Process/Thread:** `LinuxProcess` / `LinuxThread` as
  extension traits on Zircon Process/Thread
- **Filesystem:** `FileLike` trait, File, Pipe, EpollFile,
  EventFd, Stdin/Stdout, devfs (/dev/null, /dev/random,
  /dev/fb0, /dev/ttyS*, /dev/input/*)
- **Signals:** Full Linux signal model (1-64),
  SignalAction, signal delivery with arch-specific
  MachineContext
- **Networking:** TCP, UDP, Raw, Netlink sockets via
  smoltcp
- **IPC:** System V semaphores and shared memory
- **Sync:** EventBus, counting Semaphore
- **ELF Loader:** `LinuxElfLoader` with dynamic linker
  support, auxv setup

TODO: Shouldn't an ELF loader for linux be just a userspace
linux binary, and not included in linux-object?

**Workspace dependencies:** `zircon-object`, `kernel-hal`,
`drivers` (zcore-drivers)

**Status:** Actively used. Required for Linux mode.

---

### `linux-syscall/` -- Linux Syscall Dispatch

**Purpose:** Receives raw Linux syscall numbers and
arguments from userspace, dispatches them to handler
functions, and returns results. Implements ~130 syscalls.

**Syscall categories:** File I/O (~25), FD management
(~10), directories (~12), stat (~4), I/O multiplexing (~8),
process/thread (~12), signals (~8), virtual memory (~5),
time (~15), IPC (~7), networking (~16), system/misc (~20+)

**Key design:**
- `build.rs` generates `SyscallType` enum from `.h.in`
  header files (one per arch)
- All blocking syscalls are async
- Architecture-specific code paths for x86_64, aarch64,
  riscv64 (different syscall numbers and struct layouts)
- `test/` directory has C test programs for manual
  validation

TODO: Are those C tests actually used? Do they overlap with the
libc-test folder that we have started using?

**Workspace dependencies:** `zircon-object`,
`linux-object`, `kernel-hal`

TODO: Why does linux-syscall depend on zircon-object, shouldn't 
it depend fully and only on linux-object?

**Status:** Actively used. Required for Linux mode.

---

### `zircon-syscall/` -- Zircon Syscall Dispatch

**Purpose:** Implements the Fuchsia Zircon kernel ABI --
translates raw syscall numbers into calls on Zircon kernel
objects. Implements ~100+ of the 167 defined syscalls.

TODO: We need to understand the other 67 syscalls, why not implemented
and look at implementing them to bring it upto scratch. Produce an
analysis of those not implemented, by category, with an estimation of 
how much work it would be to implement them.

TODO: Need to track changes to the Zircon syscall interface made after 
this project started, to see what new changes should be included.

TODO: We should aspire to be able to run Fuschia on this kernel 
when done.

**Syscall categories:** Handles (4), objects (8), tasks
(17), channels (7), sockets (4), FIFOs (3), VMOs (11),
VMARs (5), streams (6), signals (5), ports (3), futex (4),
time (6), DDK/device (11), PCI (10), debug (5), exceptions
(3), resource (1), system (1), CPRNG (1), composite exit
(2), hypervisor (7, feature-gated)

**Key design:**
- `build.rs` generates `SyscallType` from Fuchsia's
  official `zx-syscall-numbers.h`
- Rights enforcement on every handle access
- User memory access via typed safe pointers
- Async for blocking syscalls

**Workspace dependencies:** `zircon-object`, `kernel-hal`

TODO: Clarify no deependency on zCore. Or is the dependency via
the syscall numbers, at run time?

**Status:** Actively used for Zircon mode. Less active than
Linux mode currently.

TODO: Clarify if we can boot into Zircon mode, and what to 
expect there if we can, how to check this is working in tests?

---

### `loader/` -- User Program Loader

**Purpose:** The integration hub that loads ELF binaries and
runs the user/kernel trap loop. Supports both Linux and
Zircon modes.

**Two modes:**
- **Linux** (`linux` feature): Loads Linux ELF, runs async
  trap loop dispatching Linux syscalls, handles signals
- **Zircon** (`zircon` feature): Implements Fuchsia
  `userboot` protocol, loads prebuilt userboot/vDSO, sets
  up initial channel with 15 handles

**Structure:**
- `src/linux.rs` -- Linux process lifecycle, trap loop,
  signal delivery
- `src/zircon.rs` -- Zircon userboot protocol,
  exception-based trap loop
- `examples/` -- Runnable examples for libos mode

TODO: Describe these examples more, both how to run them and
what each does.

- `tests/` -- Integration tests (busybox commands, Zircon
  userboot)

TODO: Describe these tests more and what they actually test 
and if run in make test or CI tests.

**Workspace dependencies:** `kernel-hal`, `zircon-object`,
`linux-object`, `linux-syscall`, `zircon-syscall`,
`executor`

**Status:** Actively used. Central integration point.

---

### `drivers/` -- Device Driver Framework (`zcore-drivers`)

**Purpose:** Unified device driver framework with
trait-based interfaces and concrete implementations for
multiple architectures.

TODO: Clarify why the device driver interface is not part of the kernal 
code or HAL, with implementations being here.

**Driver types:**
- **Interrupt controllers:** GIC-400 (aarch64), PLIC
  (riscv64), APIC (x86_64)
- **UARTs:** PL011, 16550 (MMIO + PMIO), Allwinner, FU740
- **Block:** VirtIO block, NVMe
- **Display:** VirtIO GPU, UEFI framebuffer
- **Network:** E1000, Realtek, loopback
- **Input:** VirtIO input
- **Mock:** SDL-based mock drivers for libos mode

TODO: Discuss which drivers are able to be used in qemu, either in
a generic aarch64 context, or a specific Raspberry Pi context.

TODO: Doe qemu offer a standard UART functionality, and if so which
driver should be used with it and how to memory map it.

**Key design:**
- `Scheme` traits define driver interfaces (BlockScheme,
  UartScheme, NetScheme, etc.)
- `DevicetreeDriverBuilder` walks FDT for auto-discovery
  (aarch64/riscv64)
- Communicates with kernel via extern "C" FFI (no
  workspace crate dependencies)

**Workspace dependencies:** None (self-contained, uses FFI
to kernel)

TODO: Clarify if drivers are then in fact running in userspace?

**Status:** Actively used. Essential for hardware interaction.

---

### `xtask/` -- Build Task Runner

**Purpose:** Implements the cargo-xtask pattern, providing
a type-safe CLI for all build, test, rootfs, and QEMU
operations.

**Key subcommands:** `qemu`, `bin`, `image`, `rootfs`,
`check-style`, `zircon-init`, `gdb`, `asm`, `libc-test`,
and more.

TODO: Describe that each of the commands are used for in
more detail.

**Structure:**
- `src/main.rs` -- CLI definition (clap derive), 21
  subcommands
- `src/build.rs` -- Core build orchestration, QEMU launch
- `src/arch.rs` -- Architecture enum, musl cross-toolchain
  resolution
- `src/linux/` -- Rootfs building, image creation, test
  deployment

**Workspace dependencies:** `z-config`

**Status:** Actively used. Default build entry point.

---

### `z-config/` -- Machine Configuration

**Purpose:** Parses `config/machine-features.toml` and
provides `MachineConfig` structs to xtask for selecting
architecture, features, and build options per machine
target.

**Defined machines:** `virt-aarch64`, `virt-riscv64`,
`virt-x86_64`, `nezha` (Allwinner D1), `cr1825` (T-HEAD),
`visionfive` (StarFive), `fu740` (SiFive)

**Workspace dependencies:** None (uses serde + toml)

**Status:** Actively used. Required by xtask.

TODO: Could this be moved into a sub directory of xtask?

---

### `third-party/executor/` -- Async Task Executor

**Purpose:** A `no_std` async task executor for bare-metal
environments. Provides `spawn()`, `run_until_idle()`, and
`sched_yield()`.

**Key detail:** Forked/vendored with local modifications
(removed trapframe dependency, architecture-specific
scheduling).

**Workspace dependencies:** None

**Status:** Actively used. Depended on by `kernel-hal`,
`loader`, and `zCore`.

TODO: Describe more how this is used at runtime.
---

### `third-party/region-alloc/` -- Region Allocator

**Purpose:** A `no_std` BTreeSet-based memory region
allocator supporting add, subtract, allocate-by-address,
and allocate-by-size with alignment.

**Workspace dependencies:** None

**Status:** Actively used. Depended on by `zircon-object`
(for VMAR address allocation).

TODO: Describe the features of the allocator that are actually
used by zircon-object

---

## Excluded Standalone Projects

These are in the repository but explicitly excluded from
the workspace via `exclude = ["zircon-user", "rboot"]`.

### `zircon-user/` -- Zircon User-Space Tests

**Purpose:** Standalone Rust project for Zircon user-space
test programs.

**Contents:** A single `hello.rs` binary. Has its own
`Cargo.toml`, `Cargo.lock`, and `rust-toolchain.toml`.

**Status:** Minimal / Legacy. Contains only a trivial
hello-world program. Likely a placeholder or relic from
early Zircon compatibility development.

TODO: Explain how this would be used and what happens in order 
to make it work. How would it be included in a built image and 
could it be invoked from the command line in some way? Is it similar to 
examples of user programs included in tests/ or examples/
of other crates.

TODO: Explore if it builds and runs, and if it can be incorporated
into Makefile and CI tests to confirm that zircon userspace programs
build and work. If so, do not exclude it from the workspace and
have it built by default. Update it to generic nightly or even
stable?

TODO: Generate some more interesting zircon user programs, that
stress more of the system and the API.

---

### `rboot/` -- x86_64 UEFI Bootloader (Git Submodule)

**Purpose:** UEFI bootloader for x86_64. Loads a kernel
ELF and boots it. Points to
`https://github.com/rcore-os/rboot.git`.

**Status:** Partial / Legacy. Only relevant for x86_64
UEFI boot, which is not the current primary development
target. Developer docs note it "is a legacy issue, will
be resolved in the future."

---

## Git Submodules

### `libc-test/` -- musl libc Test Suite (Git Submodule)

**Purpose:** The musl libc conformance test suite
(C project). Used to validate zCore's Linux syscall
compatibility.

**Points to:** `https://github.com/rcore-os/libc-test`

**Status:** Actively used in CI. The `test.yml` workflow
runs libc functional tests in QEMU via
`scripts/libc-test.sh`.

---

## Non-Crate Folders

### `.cargo/` -- Cargo Configuration

**Purpose:** Defines 17 cargo command aliases that route
through xtask (e.g., `cargo qemu` ->
`cargo run --package xtask --release -- qemu`).

**Status:** Actively used. Critical for the build workflow.

---

### `.github/` -- CI/CD Configuration

**Purpose:** GitHub Actions workflows and helper scripts.

**Workflows:**
- `build.yml` -- Format check + workspace build +
  bare-metal builds (aarch64/riscv64), runs on Ubuntu
  and macOS
- `test.yml` -- Unit tests + boot smoke test + libc
  conformance tests (aarch64), runs on Ubuntu and macOS

**Helper scripts:**
- `scripts/add-doc-index.sh` -- Documentation redirect
  page

TODO: Describe what this does more, I don't understand it.

- `scripts/make-qemu.sh` -- Build QEMU from source for CI

TODO: Do we need this, or is there a binary install of qemu
available on our target CI platforms?

**Status:** Actively used. Runs on every push and PR.

---

### `config/` -- Machine Configuration Files

**Purpose:** Contains `machine-features.toml` which defines
all supported machine targets with their architecture, PCI
support, and feature flags.

**Status:** Actively used. Read at build time by `z-config`.

TODO: Seems complicated for what it achieves. Got any ideas
on how to simplify this?

---

### `docs/` -- Documentation

**Purpose:** Project documentation including English README,

TODO: Finish translation of the main readme to english and make
chinese the backup (I thought this had been done) 

developer guidelines, and hardware-specific deployment
guides (primarily in Chinese).

**Key files:**
- `README_EN.md` -- Authoritative user-facing guide
- `for-developers.md` -- Developer conventions and
  policies
- `porting-rv64.md` -- RISC-V porting log
- `README-D1.md`, `README-C910.md`, `README-fu740.md`,
  `README-visionfive.md` -- Board-specific deployment
- `structure.svg` -- Architecture diagram

**Status:** Actively used as reference documentation.

---

### `scripts/` -- Build and Test Scripts

**Purpose:** Shell scripts for boot testing, libc testing,
and Zircon prebuilt generation.

**Key files:**
- `boot-test.sh` -- Boot smoke test (QEMU launch, wait
  for shell prompt, poweroff)
- `libc-test.sh` -- Run musl libc-test suite in QEMU,
  report pass/fail counts
- `gen-prebuilt.sh` -- Generate Zircon prebuilts from
  Fuchsia source

TODO Can you explain that about Zircon prebuilts more, 
and what Fuschia source is used from where?

- `zcore.patch` / `zircon-libos.patch` -- Fuchsia source
  patches for zCore compatibility

TODO: Explain this more, why are these patches needed?

**Status:** Actively used in CI (`boot-test.sh`,
`libc-test.sh`).

---

### `prebuilt/` -- Prebuilt Firmware

**Purpose:** Pre-built firmware binaries for booting on
various platforms.

**Contents:**
- `firmware/aarch64/` -- UEFI bootloader EFI, Boot.json,
  QEMU_EFI.fd
- `firmware/riscv/` -- OpenSBI firmware, DTBs, FIT sources
  for C910, D1, FU740, VisionFive

**Status:** Actively used. Required for QEMU boot
(QEMU_EFI.fd) and physical hardware deployment.

---

### `rootfs/` -- Root Filesystem

**Purpose:** User-space filesystem trees packed into disk
images for running inside zCore.

**Contents:**
- `aarch64/` -- busybox + 34 symlinked utilities + musl
  dynamic linker + libc-test binaries

TODO: Tell me what utilities are included, and where their
source code is, or where the binaries are found.

TODO: Describe how they are organized in the resulting FS. Will it
just be "/" (root) and then the structure here (/bin and /lib) below that?

TODO: Describe the process to build the binraies and libraries, and 
if in fact this is an output only directory... Where is the definition
of what to include in this build and what script/build tools use that
definition.

TODO: Clarify if these can be linux binaries only (when building in linux mode)
or if they are zircon binaries only (when building in zircon mode) and if
it is possible to include binaries for both and have both loaders and 
run zircon and linux binaries side by side.

TODO: Tell me if the core-utils (rust version) could be compiled 
and included here instead. Are there tests that we could run on them to test
their conformance and correct operation on the linux emulation.

TODO: Describe what to do to include a new utility in the rootfs image,
with a detailed example developers can follow.

- `riscv64/` -- busybox + 29 symlinked utilities + musl
  dynamic linker

TODO: Clarify if these need to be platform/architecture specific, or if they could
be defined just once and built for the platform being targeted by the build.

TODO: We should probably keep alive the x86_64 build, running in make and make test
and including in the target matrix in CI.

**Status:** Actively used. Built by `cargo rootfs`, packed
by `cargo image`, used by `cargo qemu`.

---

### `tools/` -- Docker Development Environment

**Purpose:** Dockerfile and scripts for building a
containerized zCore development environment (Ubuntu 20.04,
QEMU, Rust).

TODO: Should probably just rename to "docker", and then organize scripts and docker 
and other folders into a top level "tools2 folder."

**Status:** Moderately used. The Dockerfile is somewhat
dated but functional. CI does not use Docker.

---

### `ignored/` -- Build Artifacts (gitignored)

**Purpose:** Downloaded/built artifacts: cross-compilation
toolchains, busybox builds, firmware.

**Structure:**
- `origin/` -- Downloaded source archives and repos
  (busybox, musl-cross)
- `target/` -- Built artifacts per architecture

**Status:** Actively used (auto-populated by xtask). Not
in version control.

---

## Dependency Tree

### Full Crate Dependency Tree

Each crate is shown with its direct workspace dependencies
indented below it. Leaf crates (no workspace dependencies)
are at the top; the final binary is at the bottom.

```
LEAF CRATES (no workspace dependencies)
========================================
drivers/              (zcore-drivers)
  -- device driver framework
  -- links to kernel via extern "C" FFI
third-party/executor/ (executor)
  -- bare-metal async task executor
third-party/region-alloc/ (region-alloc)
  -- BTreeSet-based region/range allocator
z-config/             (z-config)
  -- machine target config parser


LEVEL 1 (depends only on leaf crates)
========================================
kernel-hal/
 +-- drivers/
 |   (always, feature "virtio"; optional
 |    features: mock, graphic, loopback)
 +-- third-party/executor/
     (bare-metal only, target_os = "none")

TODO: Describe why kernel-hal depends on drivers...

zircon-object/
 +-- kernel-hal/
 +-- third-party/region-alloc/

TODO: Confirm that zircon kernel itself doesn't provide an allocator, but leaves 
this to be implemented on top of it. Does the allocator run in the 
kernel (superviser mode) or in user space?


LEVEL 2 (depends on Level 1 + leaf crates)
========================================
linux-object/
 +-- zircon-object/    (with feature "elf")
 +-- kernel-hal/       (default-features=false)
 +-- drivers/          (with feature "virtio")
 
 TODO: Describe why linux-object depends on drivers
 

zircon-syscall/
 +-- zircon-object/
 +-- kernel-hal/       (default-features=false)


LEVEL 3 (depends on Level 2)
========================================
linux-syscall/
 +-- zircon-object/
 +-- linux-object/
 +-- kernel-hal/       (default-features=false)

TODO Describe why linux-syscall also depends on zircon-object
and not linux-object only
 

LEVEL 4 (integration hub)
========================================
loader/
 +-- kernel-hal/
 +-- zircon-object/
 +-- linux-object/     (optional, "linux")
 +-- linux-syscall/    (optional, "linux")
 +-- zircon-syscall/   (optional, "zircon")
 +-- third-party/executor/

TODO: Describe the loader operation and does it run in the kernel (supervisor)
or in userspace

TODO: Describe if the loader can load only one binary type (zircon or linux)
or both?

LEVEL 5 (final kernel binary)
========================================
zCore/ (binary)
 +-- kernel-hal/
 +-- zircon-object/
 +-- loader/
 +-- third-party/executor/
 +-- linux-object/     (optional, "linux")

TODO: Clarify where the core kernel code actually resides, is it in kernel-hal, in zircon-object
or across both of them?

TODO: When working in rust, it is possible to have trait definitions in a crate
that also implements code, and then have others augment it by implementing those traits
in separate crates that can then be bundled with the first crate. Analyze if this could
not be done here, and kernel-hal and zircon-object combined to be the zircon kernal, that 
declares traits in it's crate, that other crates withing to implement them could use.

TODO: In theory a "HAL" is an interface definition, that an implementation on a specific 
architecture and target board must implement and be supplied to the kernel, which is arch and
target board indpenedent. Contrast that with what we have here and describe the major
divergencies.
- drivers eeem to be platform indpenedent. Is that enabled by code from HAL that is platform
  dependent?
- There seems to be a mix of interface definition and implementation in kernel-hal
- Does zircon-object only implement zircon object, or does it also have core kernel functionality?
 

BUILD TOOL (not linked into the kernel)
========================================
xtask/
 +-- z-config/
```

### Visual Dependency Graph

Arrows point from dependant -> dependency
("A --> B" means A depends on B).

```
       BUILD-TIME ONLY
      +------------------------------+
      |  config/                      |
      |  machine-features.toml       |
      |         |                     |
      |         v                     |
      |   +-----------+              |
      |   | z-config  |              |
      |   +-----+-----+              |
      |         |                     |
      |         v                     |
      |   +-----------+              |
      |   |   xtask   |              |
      |   +-----------+              |
      +------------------------------+

       KERNEL RUNTIME
  +-------------------------------------+
  |                                     |
  | +-----------+ +--------+ +-------+  |
  | | executor  | |region- | |drivers|  |
  | |  (leaf)   | |alloc   | |       |  |
  | +--+----+---+ |(leaf)  | | (leaf)|  |
  |    |    |     +---+----+ +--+---+   |
  |    |    |         |         |       |
  |    |    |    +----+    +----+       |
  |    |    |    |         |            |
  |    v    |    v         v            |
  |  +------+---------------+           |
  |  |     kernel-hal       |<---+      |
  |  +---------+------------+    |      |
  |            |                 |      |
  |       +----+                 |      |
  |       |    |                 |      |
  |       v    v                 |      |
  |  +---------------+           |      |
  |  | zircon-object |<-----+    |      |
  |  +--+--------+---+      |    |      |
  |     |        |          |    |      |
  |     |   +----+------+   |    |      |
  |     |   |zircon-    |   |    |      |
  |     |   |syscall    |   |    |      |
  |     |   +-----+-----+   |    |      |
  |     |         |         |    |      |
  |     v         |         |    |      |
  |  +------------+--+      |    |      |
  |  | linux-object  |      |    |      |
  |  +-----+---------+      |    |      |
  |        |                |    |      |
  |        v                |    |      |
  |  +-------------+        |    |      |
  |  |linux-syscall|        |    |      |
  |  +-----+-------+        |    |      |
  |        |                |    |      |
  |        v                v    |      |
  |  +-------------------------+ |      |
  |  |        loader           +-+      |
  |  +------------+------------+        |
  |               |                     |
  |               v                     |
  |  +---------------------------+      |
  |  |    zCore (kernel binary)  |      |
  |  +---------------------------+      |
  |                                     |
  +-------------------------------------+
```

### Per-Crate Dependency Breakdown

#### Leaf crates (no workspace deps)

| Crate              | Notes                   |
|--------------------|-------------------------|
| `drivers`          | Self-contained. Uses    |
| (zcore-drivers)    | extern "C" FFI to       |
|                    | communicate with kernel |
| `executor`         | Vendored fork. Uses     |
|                    | only external crates    |
| `region-alloc`     | Pure data structure,    |
|                    | zero external deps      |
| `z-config`         | Uses only serde + toml  |

#### Level 1

| Crate           | Dep            | Relationship  |
|-----------------|----------------|---------------|
| `kernel-hal`    | `drivers`      | Uses driver   |
|                 |                | scheme traits |
|                 | `executor`     | Bare-metal    |
|                 |                | thread spawn  |
| `zircon-object` | `kernel-hal`   | UserContext,  |
|                 |                | PageTable,    |
|                 |                | MMUFlags      |
|                 | `region-alloc` | VMAR address  |
|                 |                | allocation    |

#### Level 2

| Crate            | Dep             | Relationship |
|------------------|-----------------|--------------|
| `linux-object`   | `zircon-object` | Extends      |
|                  |                 | Process/     |
|                  |                 | Thread       |
|                  | `kernel-hal`    | Timer,       |
|                  |                 | console,     |
|                  |                 | user ptrs    |
|                  | `drivers`       | Socket set,  |
|                  |                 | block/UART   |
| `zircon-syscall` | `zircon-object` | All Zircon   |
|                  |                 | kernel objs  |
|                  | `kernel-hal`    | User ptrs,   |
|                  |                 | timers       |

#### Level 3

| Crate           | Dep             | Relationship  |
|-----------------|-----------------|---------------|
| `linux-syscall` | `zircon-object` | Process, VMO, |
|                 |                 | Thread        |
|                 | `linux-object`  | All Linux     |
|                 |                 | abstractions  |
|                 | `kernel-hal`    | User ptrs,    |
|                 |                 | timers        |

#### Level 4

| Crate    | Dep              | Relationship       |
|----------|------------------|---------------------|
| `loader` | `kernel-hal`     | User context, VM    |
|          | `zircon-object`  | Process, Thread,    |
|          |                  | VMAR, Channel       |
|          | `linux-object`   | (optional) FS,      |
|          |                  | signals, ELF loader |
|          | `linux-syscall`  | (optional) Linux    |
|          |                  | syscall dispatch    |
|          | `zircon-syscall` | (optional) Zircon   |
|          |                  | syscall dispatch    |
|          | `executor`       | Async task runner   |

#### Level 5

| Crate    | Dep             | Relationship        |
|----------|-----------------|---------------------|
| `zCore`  | `kernel-hal`    | HAL init, config    |
|          | `zircon-object` | Page fault handling |
|          | `loader`        | linux::run() or     |
|          |                 | zircon::run()       |
|          | `executor`      | run_until_idle()    |
|          | `linux-object`  | (optional) rootfs   |

#### Build tool (separate)

| Crate   | Dep        | Relationship         |
|---------|------------|----------------------|
| `xtask` | `z-config` | Machine target       |
|         |            | resolution           |

### Transitive Dependency Chains

The longest chain (from leaf to binary):

```
drivers -> kernel-hal -> zircon-object
  -> linux-object -> linux-syscall
  -> loader -> zCore
```

A change in `drivers` can transitively affect every
crate in the project.

### Feature-Gated Dependencies

| From         | To              | Feature Gate       |
|--------------|-----------------|--------------------|
| `loader`     | `linux-object`  | `linux` feature    |
| `loader`     | `linux-syscall` | `linux` feature    |
| `loader`     | `zircon-syscall` | `zircon` feature  |
| `zCore`      | `linux-object`  | `linux` feature    |
| `kernel-hal` | `executor`      | bare-metal only    |
|              |                 | (target_os="none") |
| `zircon`     | `xmas-elf`      | `elf` feature      |
| `-object`    |                 |                    |
| `zircon`     | hypervisor mod  | `hypervisor`       |
| `-object`    |                 | (currently off)    |
| `zircon`     | hypervisor mod  | `hypervisor`       |
| `-syscall`   |                | (currently off)    |

TODO: Confirm that loader doesn't depend on zircon-object, but it does
depend on linux-object. What's the difference?

TODO: Use cargo all-features to test all these feature combinations (the
permissible ones) compile.

---

## External Dependencies

External (non-workspace) crate dependencies used across
the project, classified by role.

### Major Subsystem Libraries

These are large, complex crates that provide significant
functionality to zCore. A change or update in any of
these would have broad impact.

**`smoltcp`** (git, rev `35e833e3`)
  User-space TCP/IP network stack. Provides TCP, UDP,
  raw, and ICMP sockets. Used by `drivers`,
  `kernel-hal`, and `linux-object`. Pinned to a specific
  git revision. Features enabled: `proto-ipv4`,
  `proto-ipv6`, `proto-igmp`, `socket-raw`, `socket-udp`,
  `socket-tcp`, `socket-icmp`, `async`.

TODO: Explore tracking most recent release rather than a revision.

**`rcore-fs` family** (git, rev `1a3246b`)
  Virtual filesystem framework from the rCore project.
  Seven crates from one repo:
  - `rcore-fs` -- VFS trait definitions. Used by
    `linux-object`, `linux-syscall`, `zCore`, `xtask`.
  - `rcore-fs-sfs` -- Simple File System. Used by
    `linux-object`, `zCore`, `xtask`.
  - `rcore-fs-ramfs` -- RAM filesystem. Used by
    `linux-object`.
  - `rcore-fs-mountfs` -- Mount overlay FS. Used by
    `linux-object`.
  - `rcore-fs-devfs` -- Device FS (`/dev/null`,
    `/dev/zero`). Used by `linux-object`.
  - `rcore-fs-hostfs` -- Host filesystem passthrough
    (libos mode). Used by `loader` (dev), `zCore`.
  - `rcore-fs-fuse` -- FUSE adapter for image creation.
    Used by `xtask`.

TODO: Understand the API between the OS and the filesystem and see 
what other filesystems could be options for running on zircon or linux

**`trapframe`** (0.9.0, crates.io)
  User/kernel context save/restore and trap frame
  structures. Provides `UserContext` for entering and
  returning from user-space. Used by `kernel-hal`.
  Critical for the entire syscall entry/exit path.

**`virtio-drivers`** (git, rev `2aaf7d6`)
  VirtIO device driver implementations (block, GPU,
  input, console, network). Used by `drivers` behind
  the `virtio` feature (enabled by default).

**`futures`** (0.3, crates.io)
  Async primitives (`oneshot`, `select_biased!`,
  `FutureExt`). Used with `no_std` + `alloc` +
  `async-await` features. Used by `zircon-object`,
  `zircon-syscall`, `linux-syscall`.

**`async-std`** (1.10, crates.io)
  Full async runtime for the host OS. Provides task
  spawning, sleeping, I/O, and the `#[async_std::main]`
  / `#[async_std::test]` macros. Used in libos mode
  by `kernel-hal`, `zCore`, `drivers`; used in tests
  by `zircon-object`, `linux-syscall`, `loader`.

### Architecture-Specific Hardware Crates

These crates provide register access, instruction
wrappers, and hardware abstractions for specific CPU
architectures.

**AArch64:**
- `cortex-a` (7.2.0) -- ARM Cortex-A register access.
  Used by `kernel-hal`, `executor`.
- `tock-registers` (0.7) -- Type-safe MMIO register
  definitions. Used by `kernel-hal`, `executor`.

**RISC-V:**
- `riscv` (0.8/0.9) -- CSR access, `satp` register.
  Used by `drivers`, `kernel-hal`, `zCore`, `executor`.
  Note: two different versions in the workspace.
- `sbi-rt` (0.0.2) -- SBI runtime calls (hart start,
  system reset). Used by `kernel-hal`, `zCore`.
- `dtb-walker` (0.2.0-alpha.3) -- Device tree blob
  parsing. Used by `zCore` (riscv only).
- `page-table` (0.0.6) -- Sv39 page table types.
  Used by `zCore` (riscv only).
- `r0` (1) -- BSS zeroing utility.
  Used by `zCore` (riscv only).

**x86_64:**
- `x86_64` (0.14) -- Page tables, GDT, IDT, control
  registers. Used by `drivers`, `kernel-hal`, `executor`.
- `x86` (0.46) -- I/O ports, MSRs, segment registers.
  Used by `kernel-hal`.
- `x2apic` (0.4) -- Local APIC and I/O APIC drivers.
  Used by `drivers`, `kernel-hal`.
- `raw-cpuid` (9.0/10.2) -- CPUID instruction wrapper.
  Used by `kernel-hal`, `executor`. Two versions.
- `uefi` (0.16) -- UEFI boot services types.
  Used by `kernel-hal` (bare-metal x86_64).
- `rboot` (git, rev `ad21575`) -- UEFI bootloader
  interface (`BootInfo` struct).
  Used by `zCore` (bare-metal x86_64).
- `x86-smpboot` (git, rev `1069df3`) -- SMP AP startup.
  Used by `kernel-hal` (bare-metal x86_64).
- `acpi` (4.1) -- ACPI table parsing.
  Used by `drivers` (x86_64).

### Device and Driver Support Crates

**`device_tree`** (git, rev `2f2e55f`)
  Flattened device tree (FDT/DTB) parser. Used by
  `drivers` for device discovery on aarch64/riscv64.

**`isomorphic_drivers`** (git, rev `f7cd97a8`)
  Platform-independent driver implementations from the
  rCore ecosystem. Used by `drivers`.

**`pci`** (git, rev `8f33774b`)
  PCI bus scanning and configuration space access.
  Used by `drivers`.

**`bitmap-allocator`** (git, rev `88e871a5`)
  Bitmap-based physical frame allocator. Used by
  `drivers`, `kernel-hal` (libos), `zCore` (x86_64 +
  libos).

**`d1-pac`** (0.0.27, optional)
  Allwinner D1 peripheral access crate. Used by
  `drivers` behind `allwinner` feature.

### Synchronization and Concurrency

**`lock`** (git, kernel-sync, rev `8486b8`)
  Kernel-compatible `Mutex` / `RwLock`. The most widely
  used sync primitive across the project. Used by 7
  crates: `drivers`, `kernel-hal`, `zircon-object`,
  `zircon-syscall`, `linux-object`, `linux-syscall`,
  `zCore`.

**`spin`** (0.9)
  `Once<T>` for one-time initialization and spinlocks.
  Used by `kernel-hal`, `zCore`, `executor`.

**`lazy_static`** (1.4)
  Lazy-initialized statics. Used with `spin_no_std`
  feature in kernel crates. Used by `drivers`,
  `kernel-hal`, `zircon-object`, `linux-object`,
  `linux-syscall`, `executor`.

TODO: See if can be replaced by rust provided types, not 
sure if possible in no_std

### Utility and Glue Crates

**`log`** (0.4)
  Logging facade. The single most widely used dependency
  -- present in 9 of 12 workspace crates.

**`cfg-if`** (1.0)
  Conditional compilation macros. Used by 8 crates.

**`bitflags`** (1.3)
  Typed bitflag definitions (`Signal`, `Rights`,
  `MMUFlags`, `VmarFlags`, etc.). Used by 6 crates.

**`numeric-enum-macro`** (0.2)
  Derives numeric enums with `TryFrom`. Used for
  `SyscallType`, `ResourceKind`, `FcntlCmd`, etc.
  Used by 6 crates.

**`hashbrown`** (0.9)
  `no_std`-compatible hash map. Used by `zircon-object`
  (handle storage) and `linux-object` (fd table, futex
  table).

**`downcast-rs`** (1.2)
  Runtime downcasting for trait objects. Used by
  `zircon-object` (`Arc<dyn KernelObject>` -> concrete
  type) and `linux-object` (`dyn FileLike` ->
  `EpollFile`).

**`xmas-elf`** (0.7)
  ELF binary parser. Used by `zircon-object` (behind
  `elf` feature), `linux-object`, and `loader` (behind
  `zircon` feature).

**`bitvec`** (0.22)
  Bit-vector type for `FdSet` in select/pselect. Used
  by `linux-syscall`.

**`static_assertions`** (1.1.0)
  Compile-time struct size checks. Used by
  `linux-syscall`.

**`async-trait`** (0.1)
  Async methods in traits. Used by `linux-object`.

### Allocators

**`customizable-buddy`** (0.0.3)
  Buddy allocator for heap memory (up to 64 GiB).
  Used by `zCore` for the global heap on non-x86
  platforms.

**`buddy_system_allocator`** (0.8)
  Alternative heap allocator. Used by `zCore` on
  bare-metal x86_64 only.

### LibOS-Only Dependencies

These are used exclusively when running in library OS
mode on a host system.

**`nix`** (0.23)
  Unix API bindings (mmap, mprotect, signal). Used by
  `kernel-hal` for the mock memory subsystem.

TODO: Understand the "mock" case more.

**`tempfile`** (3)
  Temporary file creation. Used by `kernel-hal` for the
  mock physical memory backing store.

**`chrono`** (0.4)
  Date/time formatting. Used by `zCore` for
  human-readable log timestamps in libos mode.

**`sdl2`** (0.34)
  SDL2 bindings for graphics window. Used by `drivers`
  behind `mock` feature for the mock display.

### Executor-Internal Dependencies

**`unicycle`** (git, personal fork)
  Async stream utilities. Used internally by `executor`.

**`woke`** (0.0.2)
  Waker utilities. Used internally by `executor`.

**`bit-iter`** (1.0.0)
  Bit iterator. Used internally by `executor`.

### Build Tool Dependencies (xtask only)

These are used only by `xtask` and never linked into
the kernel.

**`clap`** (4.0) -- CLI argument parsing with derive
  macros.
**`os-xtask-utils`** (0.0.0) -- Wrappers around cargo,
  qemu, git, make, binutils commands.
**`shadow-rs`** (0.11) -- Build metadata embedding (git
  rev, timestamps). Also a build-dependency.
**`dircpy`** (0.3) -- Directory copying.
**`once_cell`** (1.15) -- Lazy initialization.
**`rand`** (0.8) -- Random number generation.
**`num_cpus`** (1) -- CPU count detection.

### Configuration Dependencies (z-config only)

**`serde`** (1.0) + **`serde_derive`** (1.0) --
  Serialization framework for TOML parsing.
**`toml`** (0.5.9) -- TOML file parser.

### Console and Graphics Support

**`rcore-console`** (git, rev `ca5b1bc`)
  Text console rendering on a framebuffer. Used by
  `drivers` behind `graphic` feature.

**`volatile`** (0.3)
  Volatile memory access for MMIO registers. Used by
  `drivers`.

### Dependency Source Summary

| Source     | Count | Examples                |
|------------|-------|-------------------------|
| crates.io  | 41    | log, bitflags, futures, |
|            |       | spin, clap, xmas-elf    |
| Git (rcore | 9     | rcore-fs-*, virtio-     |
| ecosystem) |       | drivers, device_tree,   |
|            |       | bitmap-allocator        |
| Git (other)| 5     | smoltcp, lock, pci,     |
|            |       | unicycle, x86-smpboot   |

TODO: Work to reduce git dependencies and move to released versions on crates.io

### Most Widely Used (by consumer count)

| Dependency    | Crates |
|---------------|--------|
| `log`         | 9      |
| `cfg-if`      | 8      |
| `lock`        | 7      |
| `bitflags`    | 6      |
| `numeric-     | 6      |
|  enum-macro`  |        |
| `lazy_static` | 6      |
| `async-std`   | 6      |
| `rcore-fs`    | 4      |

---

TODO: Move this entire libos section to a libos.md file in the root, link to it from 
README.md with a short description.

## LibOS Mode

LibOS (Library OS) mode allows zCore to run as a
**regular user-space process** on a host OS (Linux or
macOS) rather than on bare metal. It is used for rapid
development, testing, and debugging -- the kernel
compiles and starts in seconds, with full access to
host debugging tools (gdb, lldb, valgrind, strace).

TODO: Get this running and checked in ci, and connect debugger
to it, from gdb and IDE for rapid IDE based development

### How It Works

The core idea is to replace every hardware interaction
with a host OS equivalent:

| Bare Metal           | LibOS Replacement       |
|----------------------|-------------------------|
| Real RAM             | 1 GiB mmap'd temp file  |
| Hardware MMU / TLB   | mmap / munmap /         |
|                      | mprotect calls          |
| Privilege switch     | Direct function call    |
| (sysret/sret/eret)   | (run_fncall)            |
| Hardware timer IRQ   | SystemTime + async      |
|                      | task::sleep             |
| Interrupt controller | No-ops                  |
| UART serial          | Host stdin/stderr       |
| Block device + SFS   | Host filesystem         |
|                      | passthrough (HostFS)    |
| Kernel threads       | async-std tasks         |
| SMP / secondary CPUs | Single logical core     |
| Custom allocator     | std allocator + buddy   |
| `#![no_std]`         | Full `std` available    |
| Bootloader/assembly  | Normal `main()`         |

### Feature Flag Propagation

Enabling `libos` on the top-level `zCore` crate
cascades through the dependency tree:

```
zCore [libos]
 +-- kernel-hal [libos]
 |    +-- drivers [mock]
 |    +-- nix, tempfile, async-std,
 |        bitmap-allocator
 +-- loader [libos]
 |    +-- kernel-hal [libos]
 |    +-- zircon-object [aspace-separate]
 +-- async-std, chrono, rcore-fs-hostfs
```
TODO: "cargo build --features libos" doesn't work because of xtask. See if we can make it
easy to build and run the libos version.

The `libos` feature enables `std`, swaps in mock
drivers, activates the mmap-based memory backend, and
selects the HostFS filesystem.

TODO: Could some of this be done by defining "host" (libos) as a target platform, some of the
rootfs and boot stuff maybe need to be done as it is now though?

### Entry Point and Startup

In bare-metal mode, boot begins with architecture-
specific assembly (page table setup, MMU enable,
stack init). In libos mode, it is a plain `main()`:

```rust
// zCore/src/platform/libos/entry.rs
fn main() {
    crate::primary_main(kernel_hal::KernelConfig);
}
```

`KernelConfig` is an empty unit struct -- there is no
hardware configuration to pass. The `#![no_std]`
attribute is conditionally removed:

```rust
// zCore/src/main.rs
#![cfg_attr(not(feature = "libos"), no_std)]
```

The startup flow in `primary_main()`:
1. Init logging (uses `chrono` for timestamps)
2. Init buddy allocator (2 MiB static bootstrap)
3. `primary_init_early()` -- store config, create
   MockUart, start stdin reader task
4. Parse CLI args via `std::env::args()` (instead
   of bootloader command line)
5. Register 1 GiB of simulated physical memory
   with the buddy allocator
6. `primary_init()` -- init display/input if
   graphic; on macOS, register SIGSEGV handler
7. Start root process (Linux ELF or Zircon ZBI)

### Physical Memory Simulation

The mock memory subsystem creates a simulated 1 GiB
physical address space:

1. A temporary file is created via `tempfile` (e.g.,
   `/tmp/.../zcore_libos_pmem`)
2. The file is truncated to 1 GiB via `ftruncate`
3. The entire file is `mmap`'d at a fixed virtual
   address (`0x8_0000_0000`) with `MAP_SHARED`

This gives the kernel a contiguous region of memory
that behaves like physical RAM. The `phys_to_virt()`
function is simple pointer arithmetic:
`PMEM_MAP_VADDR + paddr`.

TODO: Understand why it is nmap-ed and not just a big malloc?

Frame allocation uses a `BitAlloc1M` bitmap allocator
managing page-sized (4 KiB) chunks within this region.

### Page Table Emulation

The `PageTable` struct is a **zero-sized type** -- it
holds no state. All operations delegate to the host:

- **`map(vaddr, paddr, flags)`** -- calls host `mmap`
  to map the temp file at offset `paddr` to the
  requested `vaddr` with `MAP_SHARED | MAP_FIXED`.
  This correctly simulates aliasing: two virtual
  pages mapped to the same physical frame will see
  each other's writes.
- **`unmap(vaddr)`** -- calls host `munmap`
- **`update(vaddr, paddr, flags)`** -- calls host
  `mprotect` to change permissions
- **`activate_paging()`** -- no-op (the host MMU is
  always active)
- **`flush_tlb()`** -- no-op

### User-Space Execution

On bare metal, entering user-space involves a privilege
level switch (e.g., `sysret` on x86_64, `eret` on
aarch64). In libos mode, user code runs at the **same
privilege level** as the kernel -- it is a direct
function call:

```
ctx.enter_uspace()
  -> self.0.run_fncall()   // libos: function call
  vs self.0.run()          // bare-metal: sysret/eret
```

The `trapframe` crate intercepts syscalls via a
function-call convention rather than a real trap. When
user code makes a syscall, control returns to the
`run_user` loop which dispatches it through the
`linux-syscall` or `zircon-syscall` crate.

There is no privilege isolation -- the guest user-space
and kernel share one host process address space.

### Thread and Timer Model

**Threads:** Each kernel thread is an `async-std` task.
`spawn()` calls `async_std::task::spawn(future)`.
Thread-local storage uses `async_std::task_local!`.
There is no multi-core support -- only one logical
core exists.

TODO: Why not use host OS native threads?

**Timers:** The current time comes from
`std::time::SystemTime`. Timer deadlines are
implemented by spawning an async task that calls
`async_std::task::sleep(duration)` and then fires
the callback. No hardware timer interrupts are
involved.

**Interrupts:** All interrupt-related HAL functions
are no-ops: `intr_on()`, `intr_off()`,
`wait_for_interrupt()`, `send_ipi()` all do nothing
or return immediately.

### Console I/O

The `MockUart` driver provides console I/O:
- **Input:** An async task continuously reads from
  host **stdin** via `async_std::io::stdin().read()`.
  Received bytes are buffered in a 256-byte ring.
- **Output:** `send()` and `write_str()` write to
  host **stderr** via `eprint!()`.

### Filesystem

In libos Linux mode, the root filesystem is a
**HostFS** -- a passthrough that maps VFS operations
directly to host OS filesystem calls. The root
directory is `<project>/rootfs/libos/`:

```rust
// zCore/src/fs.rs (libos path)
rcore_fs_hostfs::HostFS::new(
    rootfs.join("rootfs").join("libos")
)
```

Guest programs can read/write real files on the host
(within this directory). This is what makes libos
testing work -- tests can verify file effects from
both the guest and the host side.

In libos Zircon mode, the ZBI boot image is read from
a file path passed as a command-line argument.

TODO: Add description of how to use this mode, Makefile targets or similar
that can be used to start and connect to libos mode via debugger, both gdb
and llvm-db and those used in rustrover

### macOS-Specific Handling

On macOS x86_64, a `SIGSEGV` signal handler is
registered during `primary_init()`. It handles the
case where guest Linux binaries use `%fs`-relative
addressing for thread-local storage (common on Linux),
but macOS uses `%gs` for the same purpose.

The handler:
1. Catches SIGSEGV
2. Inspects the faulting instruction
3. If the opcode is `0x64` (`%fs` prefix), patches
   it to `0x65` (`%gs` prefix) in-place
4. Returns to retry the patched instruction

If the fault is not TLS-related, it panics with the
full register dump.

### Running LibOS Mode

**Via xtask:**
```
cargo linux-libos /bin/busybox ls -la
```
TODO: That comment fails thus:
"
error: Found argument '/bin/busybox' which wasn't expected, or isn't valid in this context

Usage: xtask linux-libos --args <ARGS>
"

This runs:
```
cargo run -p zcore --release \
  --features linux,libos -- /bin/busybox ls -la
```

TODO: That tries to build and run but fails. we need to debug it

**Via loader examples:**
```
cargo run --example linux-libos -- \
  /bin/busybox ls
cargo run --example zircon-libos -- \
  prebuilt.zbi "cmdline"
```

### Testing with LibOS

The integration tests in `loader/tests/linux.rs` use
libos mode with `#[async_std::test]`:

TODO: Confirm this works

```rust
async fn test(cmdline: &str) -> i64 {
    kernel_hal::init();
    let hostfs = HostFS::new("../rootfs/libos");
    let proc = zcore_loader::linux::run(
        args, envs, hostfs
    );
    proc.wait_for_exit().await
}
```

Tests cover basic commands (`ls`, `cat`, `uname`),
file operations (`touch`, `rm`, `mkdir`, `cp`),
and syscall unit tests (`testpipe1`, `testsem1`,
`testshm1`, `testpoll`, `testselect`, `testrandom`,
`testtime`). Host-side verification confirms file
effects are visible on the real filesystem.

Run all libos tests with:
```
cargo test -p zcore-loader
```

TODO: That fails to build. Are we using that in CI? Why failing locally?

### Architectural Limitations

Because libos mode runs in a single host process
with no privilege separation:

- **No memory isolation:** Guest user-space can
  corrupt kernel memory (acceptable for testing).
- **No real interrupts:** Async polling replaces
  interrupt-driven I/O.
- **No SMP:** Only one logical CPU core; no
  `secondary_main()`, no IPI delivery.
- **No hardware drivers:** Only mock UART, optional
  mock display (SDL2), and loopback network.
- **Syscall ABI differences:** The function-call
  convention may not exercise the exact same code
  paths as a real trap-based syscall.

Despite these limitations, libos mode exercises the
vast majority of the kernel's logic (process/thread
management, virtual memory, filesystem, networking,
signals, IPC) and is invaluable for rapid iteration.

---

## Build Artifacts and Generated Files

### Cargo Build Output: `target/`

The standard Cargo output directory. Gitignored.

| Path                              | Generator    |
|-----------------------------------|--------------|
| `target/{arch}/release/zcore`     | `cargo build`|
|   Kernel ELF for bare-metal.      | via xtask    |
|   `{arch}` is the custom target   |              |
|   triple (e.g., `aarch64`,        |              |
|   `riscv64`).                     |              |
| `target/{arch}/release/zcore.bin` | objcopy via  |
|   Stripped raw binary from ELF.   | `cargo bin`  |
|   Used for riscv64 QEMU and some  |              |
|   board targets.                  |              |
| `target/{arch}/release/build/`    | `cargo build`|
|   Build script outputs (OUT_DIR). |              |
| `target/release/`                 | `cargo build`|
|   LibOS mode build output (host   | (libos)      |
|   architecture).                  |              |
| `target/zcore.asm`                | `cargo asm`  |
|   Kernel disassembly dump via     |              |
|   `objdump -d`.                   |              |

### Filesystem Images: `zCore/*.img`

TODO: Explore if they can be generated (then used from) the
target directory to keep everything clean.

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

NOTE: Explain SFS briefly or add a link to read more 
about it.

### Rootfs Directories: `rootfs/{arch}/`

TODO: Explore if these could be moved under target 
also.

Gitignored. Built by `cargo rootfs`.

| Path                          | Contents      |
|-------------------------------|---------------|
| `rootfs/aarch64/bin/busybox`  | Statically    |
|                               | linked        |
|                               | busybox       |
| `rootfs/aarch64/bin/{sh,ls,`  | Symlinks to   |
| `cat,...}`                    | busybox       |
| `rootfs/aarch64/lib/ld-musl-` | Musl C        |
| `aarch64.so.1`               | library       |
| `rootfs/aarch64/bin/libc-`    | Compiled      |
| `test/`                      | libc-test     |
|                               | executables   |
| `rootfs/riscv64/`            | Same layout   |
|                               | for riscv64   |
| `rootfs/libos/`              | LibOS rootfs  |
|                               | (downloaded)  |

### Build Script Generated Files

| Path                           | Generator     |
|--------------------------------|---------------|
| `zCore/src/platform/riscv/`    | `zCore/`      |
| `kernel-vars.ld`              | `build.rs`    |
|   Generated linker script      |               |
|   fragment with BASE_ADDRESS   |               |
|   for riscv64. Gitignored.     |               |
| `$OUT_DIR/consts.rs`           | `linux-`      |
|   (in target build dir)        | `syscall/`    |
|   SyscallType enum from        | `build.rs`    |
|   architecture .h.in files.    |               |
| `zircon-syscall/src/consts.rs` | `zircon-`     |
|   Zircon SyscallType enum      | `syscall/`    |
|   from zx-syscall-numbers.h.   | `build.rs`    |
|   Written into source tree.    |               |
| `$OUT_DIR/shadow.rs`           | `xtask/`      |
|   (in target build dir)        | `build.rs`    |
|   Build metadata for `dump`.   | (shadow-rs)   |

TODO: Describe more what shadow.rs is for and what it contains
and what dump is and why it needs it.

### xtask Cache: `ignored/`

TODO: Not a great name, something better? Source cache
or something descriptive?

Gitignored. Auto-populated by the build system.
Total size ~600 MB when fully populated.

**Downloaded origins (`ignored/origin/`):**

| Path                            | Contents      |
|---------------------------------|---------------|
| `origin/archs/aarch64/`        | UEFI firmware |
| `Aarch64_firmware.zip`         | archive       |
| `origin/archs/riscv64/`        | musl cross-   |
| `riscv64-linux-musl-cross.tgz` | compiler      |
|                                 | (~103 MB)     |
| `origin/archs/x86_64/`         | Zircon        |
| `prebuilt.tar.xz`              | prebuilts     |
| `origin/archs/libos/`          | LibOS rootfs  |
| `rootfs-libos.tar.gz`          | archive       |
| `origin/repos/busybox/`        | Cloned        |
|                                 | busybox       |
|                                 | source repo   |
| `origin/repos/ffmpeg/`         | Cloned FFmpeg |
|                                 | (optional)    |
| `origin/repos/opencv/`         | Cloned OpenCV |
|                                 | (optional)    |

TODO: Describe each one a bit more.

**Built/extracted outputs (`ignored/target/`):**

TODO: Analyze if these could be built under target
also?

| Path                            | Contents      |
|---------------------------------|---------------|
| `target/aarch64/busybox/`      | Compiled      |
|                                 | busybox for   |
|                                 | aarch64       |
|                                 | (statically   |
|                                 | linked,       |
|                                 | ~53 MB with   |
|                                 | build tree)   |
| `target/aarch64/firmware/`     | Extracted      |
|                                 | QEMU_EFI.fd,  |
|                                 | bootloader,   |
|                                 | Boot.json     |
| `target/riscv64/busybox/`      | Compiled      |
|                                 | busybox for   |
|                                 | riscv64       |
|                                 | (dynamically  |
|                                 | linked)       |
| `target/riscv64/riscv64-`      | Extracted      |
| `linux-musl-cross/`            | GCC 11.2.1    |
|                                 | cross-        |
|                                 | compiler      |
|                                 | toolchain     |
|                                 | (~357 MB)     |
| `target/{arch}/ffmpeg/`        | FFmpeg build   |
|                                 | (optional)    |
| `target/{arch}/opencv/`        | OpenCV build   |
|                                 | (optional)    |

### Other Generated Artifacts

| Path                           | Generator     |
|--------------------------------|---------------|
| `zCore/disk/`                  | Build system  |
|   EFI boot disk for aarch64    | (aarch64 UEFI |
|   UEFI boot. Contains          | path). Git-   |
|   bootaa64.efi and Boot.json.  | ignored.      |
| `zCore/zcore.bin.gz`           | Makefile       |
|   Gzipped kernel for SiFive    | (fu740 build) |
|   FU740 board.                 |               |
| `zCore/zcore-fu740.itb`        | mkimage        |
|   FIT image for FU740 U-Boot.  | (fu740 build) |
| `zCore/uImageC910`             | mkimage        |
|   uImage for T-HEAD C910.     | (c910 build)  |

### What `make clean` Removes

- **`make clean`**: `cargo clean` (removes
  `target/`), `rm -f *.asm`, `rm -rf rootfs`,
  `rm -rf zCore/disk`, all `*.img` and `*.bin`
  under `zCore/`
- **`make cleanup`**: Above + `rm -rf ignored/target`
  (removes extracted toolchains and busybox builds)
- **`make clean-everything`**: Above +
  `rm -rf ignored` (removes ALL downloads, cloned
  repos, and build caches)

---

## Usage Status Summary

| Folder                  | Status       |
|-------------------------|--------------|
| `zCore/`                | **Active**   |
|   Main kernel binary    |              |
| `kernel-hal/`           | **Active**   |
|   Core infrastructure   |              |
| `zircon-object/`        | **Active**   |
|   Foundation for both   |              |
|   OS personalities      |              |
| `linux-object/`         | **Active**   |
|   Required for Linux    |              |
| `linux-syscall/`        | **Active**   |
|   Required for Linux    |              |
| `zircon-syscall/`       | **Active**   |
|   Required for Zircon   |              |
| `loader/`               | **Active**   |
|   Integration hub       |              |
| `drivers/`              | **Active**   |
|   Essential for HW      |              |
| `xtask/`                | **Active**   |
|   Primary build tool    |              |
| `z-config/`             | **Active**   |
|   Build-time config     |              |
| `third-party/executor/` | **Active**   |
|   Async runtime         |              |
| `third-party/region-`   | **Active**   |
| `alloc/`                |              |
|   Memory regions        |              |
| `config/`               | **Active**   |
|   Machine definitions   |              |
| `scripts/`              | **Active**   |
|   CI test scripts       |              |
| `.cargo/`               | **Active**   |
|   Cargo aliases         |              |
| `.github/`              | **Active**   |
|   CI workflows          |              |
| `prebuilt/`             | **Active**   |
|   Firmware binaries     |              |
| `rootfs/`               | **Active**   |
|   User-space FS         |              |
| `docs/`                 | **Active**   |
|   Documentation         |              |
| `libc-test/`            | **Active**   |
|   CI conformance tests  |              |
| `ignored/`              | **Active**   |
|   Auto-gen artifacts    |              |
| `tools/`                | **Moderate** |
|   Docker env, dated     |              |
| `rboot/`                | **Legacy**   |
|   x86_64 only           |              |
| `zircon-user/`          | **Legacy**   |
|   Single hello.rs       |              |

TODO: So let's review resurecting x86_64, which might bring
rboot back into active use. Goal is to create a bootable image
with linux subsystem for x86_64, which we can try on real hardware.

TODO: Zircon-user is commented above. It would be great to know it works
and it, or similar could form part of a minimal test that zircon user 
programs can be built and ran.