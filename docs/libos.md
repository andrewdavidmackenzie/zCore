# LibOS Mode Guide

## LibOS Mode

LibOS (Library OS) mode allows zCore to run as a **regular user-space process**
on a host OS (Linux or macOS) rather than on bare metal. It is used for rapid
development, testing, and debugging -- the kernel compiles and starts in
seconds, with full access to host debugging tools (gdb, lldb, valgrind,
strace).

Fixing and integrating libos into CI is tracked in
[#80](https://github.com/andrewdavidmackenzie/zCore/issues/80).
For debugging, run the libos binary directly under a
debugger (`lldb -- target/release/zcore /bin/busybox
sh`). For RustRover, add a Run Configuration pointing
to the zCore binary with `--features linux,libos`. No
QEMU needed.

### How It Works

The core idea is to replace every hardware interaction with a host OS
equivalent:

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

Enabling `libos` on the top-level `zCore` crate cascades through the dependency
tree:

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
The issue is that `default-members = ["xtask"]` in root Cargo.toml, so `cargo
build` builds xtask (which doesn't have a `libos` feature). Fix: `cargo build
-p zcore --features linux,libos`. Or use `cargo linux-libos` (the xtask alias).
The build failure itself needs debugging
separately. See [#80](https://github.com/andrewdavidmackenzie/zCore/issues/80).


The `libos` feature enables `std`, swaps in mock drivers, activates the mmap-
based memory backend, and selects the HostFS filesystem.

Partially. Rust's `cfg(target_os)` already distinguishes bare-metal (`target_os
= "none"`) from hosted. Some `#[cfg(feature = "libos")]` checks could be
replaced with `#[cfg(not(target_os = "none"))]` (and some already are). But
`libos` also controls dependency selection (async-std vs executor,
nix/tempfile, HostFS) which must be features. A custom target spec for "libos"
isn't practical because the host target (x86_64-unknown-linux-gnu) IS the
target. The feature flag approach is appropriate here.


### Entry Point and Startup

In bare-metal mode, boot begins with architecture- specific assembly (page
table setup, MMU enable, stack init). In libos mode, it is a plain `main()`:

```rust
// zCore/src/platform/libos/entry.rs
fn main() {
    crate::primary_main(kernel_hal::KernelConfig);
}
```

`KernelConfig` is an empty unit struct -- there is no hardware configuration to
pass. The `#![no_std]` attribute is conditionally removed:

```rust
// zCore/src/main.rs
#![cfg_attr(not(feature = "libos"), no_std)]
```

The startup flow in `primary_main()`: 1. Init logging (uses `chrono` for
timestamps) 2. Init buddy allocator (2 MiB static bootstrap) 3.
`primary_init_early()` -- store config, create MockUart, start stdin reader
task 4. Parse CLI args via `std::env::args()` (instead of bootloader command
line) 5. Register 1 GiB of simulated physical memory with the buddy allocator
6. `primary_init()` -- init display/input if graphic; on macOS, register
SIGSEGV handler 7. Start root process (Linux ELF or Zircon ZBI)

### Physical Memory Simulation

The mock memory subsystem creates a simulated 1 GiB physical address space:

1. A temporary file is created via `tempfile` (e.g.,
`/tmp/.../zcore_libos_pmem`) 2. The file is truncated to 1 GiB via `ftruncate`
3. The entire file is `mmap`'d at a fixed virtual address (`0x8_0000_0000`)
with `MAP_SHARED`

This gives the kernel a contiguous region of memory that behaves like physical
RAM. The `phys_to_virt()` function is simple pointer arithmetic:
`PMEM_MAP_VADDR + paddr`.

Physical memory is `mmap`'d (not `malloc`'d) for three reasons: (1) `MAP_FIXED`
gives a deterministic address (`0x8_0000_0000`) -- malloc returns arbitrary
addresses. (2) `MAP_SHARED` + file-backed enables aliasing: the same file
offset can be mmap'd at multiple virtual addresses with different permissions,
simulating page table mappings. (3) `mprotect()` enables per-page
read/write/execute permissions, simulating MMU protection bits. `malloc` can't
do any of these.

Frame allocation uses a `BitAlloc1M` bitmap allocator managing page-sized (4
KiB) chunks within this region.

### Page Table Emulation

The `PageTable` struct is a **zero-sized type** -- it holds no state. All
operations delegate to the host:

- **`map(vaddr, paddr, flags)`** -- calls host `mmap`
to map the temp file at offset `paddr` to the requested `vaddr` with
`MAP_SHARED | MAP_FIXED`. This correctly simulates aliasing: two virtual pages
mapped to the same physical frame will see each other's writes.
- **`unmap(vaddr)`** -- calls host `munmap`
- **`update(vaddr, paddr, flags)`** -- calls host
  `mprotect` to change permissions
- **`activate_paging()`** -- no-op (the host MMU is
  always active)
- **`flush_tlb()`** -- no-op

### User-Space Execution

On bare metal, entering user-space involves a privilege level switch (e.g.,
`sysret` on x86_64, `eret` on aarch64). In libos mode, user code runs at the
**same privilege level** as the kernel -- it is a direct function call:

```
ctx.enter_uspace()
  -> self.0.run_fncall()   // libos: function call
  vs self.0.run()          // bare-metal: sysret/eret
```

The `trapframe` crate intercepts syscalls via a function-call convention rather
than a real trap. When user code makes a syscall, control returns to the
`run_user` loop which dispatches it through the `linux-syscall` or `zircon-
syscall` crate.

There is no privilege isolation -- the guest user-space and kernel share one
host process address space.

### Thread and Timer Model

**Threads:** Each kernel thread is an `async-std` task. `spawn()` calls
`async_std::task::spawn(future)`. Thread-local storage uses
`async_std::task_local!`. There is no multi-core support -- only one logical
core exists.

Because the entire kernel is built on async/await. Kernel "threads" are
`Future`s, not OS threads. The spawn API takes `impl Future<Output=()>`, not a
closure. On bare metal, the custom executor polls these futures cooperatively.
Using `async_std::task::spawn` in libos provides the same async model backed by
a host thread pool. This ensures kernel code behaves identically in both modes.
OS threads would require fundamentally different synchronization (preemptive vs
cooperative) and break the abstraction.


**Timers:** The current time comes from `std::time::SystemTime`. Timer
deadlines are implemented by spawning an async task that calls
`async_std::task::sleep(duration)` and then fires the callback. No hardware
timer interrupts are involved.

**Interrupts:** All interrupt-related HAL functions are no-ops: `intr_on()`,
`intr_off()`, `wait_for_interrupt()`, `send_ipi()` all do nothing or return
immediately.

### Console I/O

The `MockUart` driver provides console I/O:
- **Input:** An async task continuously reads from
host **stdin** via `async_std::io::stdin().read()`. Received bytes are buffered
in a 256-byte ring.
- **Output:** `send()` and `write_str()` write to
  host **stderr** via `eprint!()`.

### Filesystem

In libos Linux mode, the root filesystem is a **HostFS** -- a passthrough that
maps VFS operations directly to host OS filesystem calls. The root directory is
`<project>/rootfs/libos/`:

```rust
// zCore/src/fs.rs (libos path)
rcore_fs_hostfs::HostFS::new(
    rootfs.join("rootfs").join("libos")
)
```

Guest programs can read/write real files on the host (within this directory).
This is what makes libos testing work -- tests can verify file effects from
both the guest and the host side.

In libos Zircon mode, the ZBI boot image is read from a file path passed as a
command-line argument.

Once libos build is fixed, usage would be: **Build**: `cargo build -p zcore
--release
--features linux,libos` **Run**: `target/release/zcore /bin/busybox sh` **Debug
with lldb**: `lldb -- target/release/zcore /bin/busybox sh` **Debug with gdb**:
`gdb --args target/release/zcore /bin/busybox sh` **RustRover**: Add Run
Configuration with executable=`target/release/zcore`, args= `/bin/busybox sh`,
env=`RUST_LOG=info`. No QEMU needed -- it's a native process.
See [#80](https://github.com/andrewdavidmackenzie/zCore/issues/80).


### macOS-Specific Handling

On macOS x86_64, a `SIGSEGV` signal handler is registered during
`primary_init()`. It handles the case where guest Linux binaries use
`%fs`-relative addressing for thread-local storage (common on Linux), but macOS
uses `%gs` for the same purpose.

The handler: 1. Catches SIGSEGV 2. Inspects the faulting instruction 3. If the
opcode is `0x64` (`%fs` prefix), patches it to `0x65` (`%gs` prefix) in-place
4. Returns to retry the patched instruction

If the fault is not TLS-related, it panics with the full register dump.

### Running LibOS Mode

**Via xtask:**
```
cargo linux-libos --args "/bin/busybox sh"
```
Note: args must be quoted as a single string after `--args`. The xtask CLI uses
clap with a `--args` flag, not positional arguments.

This runs:
```
cargo run -p zcore --release \
  --features linux,libos -- /bin/busybox ls -la
```

Note: the libos build is currently broken
(see [#80](https://github.com/andrewdavidmackenzie/zCore/issues/80)).
Likely causes: (1) nightly-only features that have
changed API, (2) stale dependency versions,
(3) missing `rootfs/libos/` directory (needs
`cargo libos-libc-test` first to download it).


**Via loader examples:**
```
cargo run --example linux-libos -- \
  /bin/busybox ls
cargo run --example zircon-libos -- \
  prebuilt.zbi "cmdline"
```

### Testing with LibOS

The integration tests in `loader/tests/linux.rs` use libos mode with
`#[async_std::test]`:

Not confirmed -- depends on fixing the libos build first (see TODO above).
These tests also require `rootfs/libos/` to be populated. They are NOT
currently in CI (`test.yml` runs QEMU-
based tests only). See [#80](https://github.com/andrewdavidmackenzie/zCore/issues/80).


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

Tests cover basic commands (`ls`, `cat`, `uname`), file operations (`touch`,
`rm`, `mkdir`, `cp`), and syscall unit tests (`testpipe1`, `testsem1`,
`testshm1`, `testpoll`, `testselect`, `testrandom`, `testtime`). Host-side
verification confirms file effects are visible on the real filesystem.

Run all libos tests with:
```
cargo test -p zcore-loader
```

Not in CI. The `test.yml` workflow runs `cargo test --no-fail-fast` which
builds the workspace default (xtask), not zcore-loader specifically. The libos
tests require `--features linux,libos` and `rootfs/libos/` populated. Failing
locally is expected until the libos build is fixed. Covered by the libos fix
issue candidate above.


### Architectural Limitations

Because libos mode runs in a single host process with no privilege separation:

- **No memory isolation:** Guest user-space can
  corrupt kernel memory (acceptable for testing).
- **No real interrupts:** Async polling replaces
  interrupt-driven I/O.
- **No SMP:** Only one logical CPU core; no
  `secondary_main()`, no IPI delivery.
- **No hardware drivers:** Only mock UART, optional
  mock display (SDL2), and loopback network.
- **Syscall ABI differences:** The function-call
convention may not exercise the exact same code paths as a real trap-based
syscall.

Despite these limitations, libos mode exercises the vast majority of the
kernel's logic (process/thread management, virtual memory, filesystem,
networking, signals, IPC) and is invaluable for rapid iteration.

---

## Build Artifacts and Generated Files
