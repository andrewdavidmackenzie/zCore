# HAL Architecture and Design

This document describes the Hardware Abstraction Layer (HAL) design in zCore,
including its macro-based dispatch system, module structure, and known
architectural issues.

For the overall project architecture, see
[architecture.md](architecture.md).
For boot sequence details, see [boot-process.md](boot-process.md).

---

## Purpose

The `kernel-hal` crate provides a unified, architecture-independent interface
for all hardware interaction. It abstracts differences between three CPU
architectures (aarch64, riscv64, x86_64) and two execution modes (bare-metal vs
libos).

---

## Macro-Based Dispatch System

The HAL uses `hal_fn_def!` / `hal_fn_impl!` macros (defined in `kernel-
hal/src/macros.rs`) instead of standard Rust traits.

### How It Works

`hal_fn_def!` declares the HAL interface. For each module, it: 1. Creates a
hidden trait `__HalTrait` with the function signatures 2. Creates a zero-sized
struct `__HalImpl` 3. Exports public free functions that delegate to
`__HalImpl` methods

`hal_fn_impl!` provides concrete implementations for `__HalImpl` by
implementing the `__HalTrait` trait. Used in `bare/` and `libos/` to provide
platform-specific implementations.

### Example

```
hal_fn_def! {
    mod thread {
        fn spawn(...);
    }
}
// Creates: thread::__HalTrait
//          thread::__HalImpl
//          thread::spawn() -> __HalImpl::spawn()

hal_fn_impl! {
    impl mod thread {
        fn spawn(...) {
            executor::spawn(future);
        }
    }
}
// Implements: __HalTrait for __HalImpl
```

### Comparison with Trait-Based Approach

| Aspect      | Macro System          | Trait-Based     |
|-------------|-----------------------|-----------------|
| Call syntax | `kernel_hal::spawn()` | `hal.spawn()`   |
|             | (free functions)      | or `T::spawn()` |
| Dispatch    | Monomorphized         | Could be static |
|             | (compile-time)        | or dynamic      |
| Overhead    | Zero (no vtable)      | Zero if static  |
| IDE support | Opaque -- hard        | IDE-friendly    |
|             | to navigate           |                 |
| Generics    | No type param         | Requires `P`    |
|             | threading             | parameter       |

A trait-based alternative could use a generic `Platform` type parameter:
`kernel_hal::Hal<P: Platform>`. Each platform would implement `Platform`. The
cost: every type touching the HAL needs a `P` parameter, which can be verbose.
The macro approach avoids this by using a single hidden impl struct.

Both approaches are valid. The macro system is non-standard but pragmatic. See
[#78](https://github.com/andrewdavidmackenzie/zCore/issues/78)
for the evaluation of migrating to traits.

---

## HAL Interface Modules

The complete interface is declared in `kernel-hal/src/hal_fn.rs`:

### `boot`
Init sequences, command line, init RAM disk.
- `primary_init_early(config, handler)` -- store config and kernel handler
- `primary_init()` -- full HAL initialization
- `secondary_init()` -- secondary core init
- `cmdline()` -- kernel command line string
- `init_ram_disk()` -- initrd bytes

### `cpu`
- `cpu_id()` -- current CPU/hart ID
- `cpu_frequency()` -- CPU clock frequency
- `reset()` -- system reset/shutdown

### `mem` (Physical Memory)
- `phys_to_virt(paddr)` -- address conversion
- `virt_to_phys(vaddr)` -- reverse conversion
- `free_pmem_regions()` -- enumerate free physical memory
- `pmem_read/write/zero/copy()` -- direct physical memory access
- `frame_flush()` -- flush frame from cache

The `mem` module deals with **physical** memory operations. Address conversion,
raw read/write of physical memory, and cache management. These operations do
not touch page tables.

### `vm` (Virtual Memory / Page Tables)
- `current_vmtoken()` -- read current page table root (CR3/TTBR/SATP)
- `activate_paging(vmtoken)` -- switch page tables
- `flush_tlb(vaddr)` -- TLB invalidation
- `pt_clone_kernel_space()` -- copy kernel page table entries

The `vm` module deals with **virtual memory** and page table management.
Activate address spaces, flush TLB, clone kernel mappings.

The `mem`/`vm` split follows standard OS convention: physical memory operations
don't involve page tables; page table operations don't directly read/write
physical content.

### `interrupt`
- `wait_for_interrupt()` -- WFI/HLT
- `intr_on/off/get()` -- enable/disable/query
- IRQ mask/unmask/configure/register/handle
- MSI alloc/free
- IPI send/receive

### `thread`
- `spawn(future)` -- spawn an async task
- `set/get_current_thread()` -- thread-local tracking

Thread spawning is in the HAL because it differs fundamentally between
platforms: bare-metal uses `executor::spawn()` (custom bare-metal async
executor with per-CPU run queues), while libos uses `async_std::task::spawn()`
(host OS thread pool). Also `set/get_current_thread` uses per-CPU static arrays
on bare-metal vs `task_local!` on libos.

### `timer`
- `timer_enable()` -- start hardware timer
- `timer_now()` -- current monotonic time
- `deadline_after(duration)` -- compute deadline
- `timer_set(deadline, callback)` -- set timer
- `timer_tick()` -- handle timer interrupt

### `rand`
- `fill_random(buf)` -- fill buffer with random bytes (x86: rdrand, others:
  PRNG)

### `vdso`
- `vdso_constants()` -- Zircon vDSO constants

The vDSO (virtual Dynamic Shared Object) is a small shared library the kernel
maps into every process. It lets userspace call certain kernel functions (e.g.,
get current time) without a syscall, by reading kernel-maintained data
directly. Both real Zircon/ Fuchsia and Linux use this mechanism. It is not a
hack -- it's a well-established OS pattern for high-frequency calls where
syscall overhead matters.

### `console`
- `console_write_early(s)` -- pre-driver console output

---

## Directory Structure

```
kernel-hal/src/
  lib.rs ............... Selects bare vs libos backend
  macros.rs ............ hal_fn_def!/hal_fn_impl! macros
  hal_fn.rs ............ Complete HAL interface declaration
  kernel_handler.rs .... KernelHandler trait (callbacks)
  config.rs ............ KernelConfig re-export
  drivers.rs ........... Device registry + FFI glue

  common/ .............. Shared types (both modes)
    addr.rs            PhysAddr/VirtAddr aliases
    context.rs         UserContext wrapper
    defs.rs            HalError, MMUFlags, PAGE_SIZE
    future.rs          Async futures
    ipi.rs             Inter-processor interrupt queues
    mem.rs             PhysFrame (RAII frame wrapper)
    thread.rs          sleep_until, yield_now
    user.rs            UserPtr<T,P> safe wrappers
    vdso.rs            VdsoConstants structure
    vm.rs              GenericPageTable trait

  bare/ ................ Bare-metal backend
    boot.rs            Init sequence impl
    mem.rs             phys_to_virt, pmem read/write
    thread.rs          spawn via executor crate
    timer.rs           Timer via naive-timer
    net.rs             Loopback network (smoltcp)
    arch/aarch64/      ARM64 specifics
      config.rs        KernelConfig struct
      cpu.rs           MPIDR_EL1, PSCI reset
      drivers.rs       GIC-400 + PL011 + VirtIO init
      interrupt.rs     DAIF, GIC IRQ handling
      mem.rs           Free memory regions
      timer.rs         CNTPCT_EL0 generic timer
      trap.rs          Exception dispatch
      vm.rs            4-level page table, TTBR
    arch/riscv/        RISC-V specifics (similar)
    arch/x86_64/       x86_64 specifics (similar)

  libos/ ............... LibOS backend
    boot.rs            Init (creates MockUart)
    config.rs          KernelConfig = unit struct
    cpu.rs             cpu_id = thread ID
    drivers.rs         Mock UART/display/input
    dummy.rs           DummyKernelHandler
    interrupt.rs       All no-ops
    mem.rs             mmap-backed physical memory
    mock_mem.rs        MockMemory (tmpfile + mmap)
    thread.rs          spawn via async_std
    timer.rs           SystemTime + task::sleep
    vm.rs              PageTable via mmap/munmap
    macos.rs           %fs/%gs TLS signal handler

  utils/ ............... Utility data structures
    init_once.rs       One-shot initialization
    lazy_init.rs       Lazy init with DerefMut
    mpsc_queue.rs      Lock-free MPSC queue
    page_table.rs      Generic multi-level page table
```

---

## KernelHandler Callback Pattern

The `KernelHandler` trait (`kernel-hal/src/kernel_handler.rs`) provides
callbacks FROM the HAL INTO the kernel. It has 4 methods:

- `frame_alloc()` -- allocate a physical frame
- `frame_alloc_contiguous(count, align)` -- allocate contiguous frames
- `frame_dealloc(paddr)` -- free a frame
- `handle_page_fault(vaddr)` -- handle a page fault

This exists because `kernel-hal` (a library crate) cannot depend on `zCore`
(the binary crate) -- that would be a circular dependency. But the HAL needs to
allocate physical frames (managed by zCore's allocator). The solution is
dependency inversion:

1. HAL defines the `KernelHandler` trait 2. Kernel implements it
(`ZcoreKernelHandler` in `zCore/src/handler.rs`) 3. Kernel passes `&'static
ZcoreKernelHandler` during `primary_init_early()` 4. HAL stores it in a global
`InitOnce<&dyn KernelHandler>` 5. HAL code (e.g., `PhysFrame::new()`) calls
`KHANDLER.frame_alloc()`

The name "KernelHandler" is vague -- better names would be `KernelCallbacks` or
`HalToKernelBridge`.

Possible simplifications (see
[#78](https://github.com/andrewdavidmackenzie/zCore/issues/78)):
- Move the allocator into kernel-hal itself (requires restructuring
  `#[global_allocator]`)
- Create a `kernel-alloc` crate both can depend on
- Use function pointers instead of a trait

---

## Platform Code Split

Platform-dependent code is split between two locations:

**`zCore/src/platform/`** -- Pre-HAL bootstrap:
- Assembly boot (page tables, MMU enable)
- Linker scripts (`.ld`)
- Binary crate entry points (`_start`, `rust_main`)
- Platform constants

**`kernel-hal/src/bare/arch/`** -- Post-boot runtime:
- Interrupt handling and dispatch
- Timer management
- Trap/exception handling
- Page table implementation
- Driver initialization

The split exists because linker scripts and `#[no_mangle]` entry points MUST be
in the final binary crate (Rust requirement). However, `entry.rs` and
`consts.rs` could potentially be moved into kernel-hal. See
[#77](https://github.com/andrewdavidmackenzie/zCore/issues/77).

---

## Architectural Observations

The current HAL design has several divergences from a clean HAL model:

1. **Drivers are platform-independent.** They use abstract bus I/O (volatile
read/write) and the HAL provides platform-specific glue (phys_to_virt, DMA
alloc) via FFI. This part is clean.

2. **kernel-hal mixes interface and implementation.** `hal_fn.rs` defines the
interface, `bare/` and `libos/` provide implementations, but `common/` has
shared types AND logic (futures, user pointer validation, PhysFrame RAII). The
shared logic should arguably be in the kernel, not the HAL.

3. **The `libos` feature is cross-cutting.** It appears ~44 times across 15
files, affecting code outside the HAL (main.rs, fs.rs, utils.rs, logging.rs,
loader). About 30% could be eliminated with better abstraction; the rest is
inherent to the std-vs-no_std divide.

4. **`zircon-object` contains core kernel functionality** beyond just Zircon
objects -- process scheduling, memory management, IPC. See
   [#96](https://github.com/andrewdavidmackenzie/zCore/issues/96).
