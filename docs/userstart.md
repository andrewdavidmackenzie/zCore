# userstart: The First Userspace Process

This document describes how the first userspace process works in the original
Fuchsia/Zircon project, and how zCore should implement it. For the kernel-side
boot sequence that leads up to this point, see [boot-process.md](boot-process.md).

---

## Overview

In Fuchsia, the Zircon kernel launches a single userspace process called
**userboot**. That process then bootstraps everything else -- drivers,
filesystems, networking, the component framework -- all in userspace.

In zCore, the equivalent is called **userstart**. It lives in
`loader/src/zircon.rs` and is the kernel-side code that creates and launches
the first userspace process.

The distinction matters: **userboot is part of the kernel image but runs in
userspace**. It is compiled as a userspace ELF binary and embedded directly
into the kernel image at build time. The kernel does not need a filesystem to
find it -- it knows the offset of the userboot ELF within its own image.

---

## The Fuchsia Boot Chain

The boot chain has three stages. Each runs in userspace except the kernel
itself.

```text
  Zircon Kernel (kernel mode)
       |
       |  Embeds userboot ELF at build time.
       |  Creates root Job, Process, Thread.
       |  Maps userboot ELF + vDSO into the process.
       |  Packs 15 handles into a bootstrap channel.
       |  Starts the thread.
       |
       v
  userboot (user mode)
       |
       |  Receives bootstrap channel with kernel handles.
       |  Parses the ZBI (Zircon Boot Image).
       |  Decompresses bootfs into a VMO.
       |  Loads bin/component_manager from bootfs.
       |  Serves as a dynamic linker (loader service).
       |  Exits when the loader service channel is closed.
       |
       v
  component_manager (user mode)
       |
       |  Sets itself as job-critical.
       |  Reads runtime config from bootfs.
       |  Builds the component framework environment.
       |  Starts the root component.
       |  Bootstraps all of Fuchsia.
       |
       v
  ... rest of Fuchsia (drivers, services, apps)
```

### Source locations in the Fuchsia tree

| Component | Location | Description |
|-----------|----------|-------------|
| Kernel-side launcher | [zircon/kernel/lib/userabi/userboot.cc](https://fuchsia.googlesource.com/fuchsia/+/refs/heads/main/zircon/kernel/lib/userabi/userboot.cc) | Kernel C++ code that creates the userboot process |
| userboot program | [zircon/kernel/lib/userabi/userboot/main.rs](https://fuchsia.googlesource.com/fuchsia/+/refs/heads/main/zircon/kernel/lib/userabi/userboot/main.rs) | First userspace process (Rust, was C++) |
| component_manager | [src/sys/component_manager/src/main.rs](https://fuchsia.googlesource.com/fuchsia/+/refs/heads/main/src/sys/component_manager/src/main.rs) | First "real" Fuchsia program |

---

## What the Zircon Kernel Does (Kernel Side)

The kernel's only job with respect to bootstrapping is to get userboot running.
After that, the kernel just services syscalls. The kernel-side code in
`userboot.cc` does the following:

1. **Creates the root Job** -- the ancestor of all jobs in the system.

2. **Creates a Process and Thread** for userboot.

3. **Maps the embedded userboot ELF** into the process address space. The ELF
   binary was baked into the kernel image at build time. The kernel has just
   enough code to parse ELF headers and map segments -- it is not a
   general-purpose ELF loader.

4. **Maps the vDSO** (virtual Dynamic Shared Object) into the process. The
   vDSO is how all Zircon userspace programs make syscalls. It contains the
   actual `syscall` / `svc` instructions. Userspace programs never execute
   trap instructions directly -- they call vDSO functions.

5. **Creates a bootstrap channel** and packs it with handles. The kernel
   writes a `MessagePacket` containing 15 handles and the kernel command line
   onto one end of the channel, then passes the other end to userboot as the
   process start handle.

6. **Starts the thread** at the ELF entry point.

After this, the kernel is done with bootstrapping. It enters its normal
syscall servicing loop.

### The 15 Bootstrap Handles

The kernel packs these handles onto the bootstrap channel. The handle indices
are defined in
[`zircon/kernel/lib/userabi/include/lib/userabi/userboot.h`](https://fuchsia.googlesource.com/fuchsia/+/refs/heads/main/zircon/kernel/lib/userabi/include/lib/userabi/userboot.h):

| Index | Name | Object Type | Purpose |
|-------|------|-------------|---------|
| 0 | `PROC_SELF` | Process | Handle to userboot's own process |
| 1 | `VMARROOT_SELF` | VMAR | Handle to userboot's root VMAR |
| 2 | `ROOTJOB` | Job | The root job (ancestor of all jobs) |
| 3 | `ROOTRESOURCE` | Resource | Root resource (access to hardware) |
| 4 | `ZBI` | VMO | The Zircon Boot Image data |
| 5 | `FIRSTVDSO` | VMO | Full vDSO image |
| 6 | `FIRSTVDSO+1` | VMO | vDSO variant (test1) |
| 7 | `FIRSTVDSO+2` | VMO | vDSO variant (test2) |
| 8 | `CRASHLOG` | VMO | Crash log from previous boot |
| 9 | `COUNTER_NAMES` | VMO | Kernel counter descriptor table |
| 10 | `COUNTERS` | VMO | Kernel counter arena |
| 11-14 | `INSTRUMENTATION` | VMO | Profiling / instrumentation data |

The message data contains the kernel command line as NUL-separated key=value
pairs.

---

## What userboot Does (Userspace)

Source: [`zircon/kernel/lib/userabi/userboot/main.rs`](https://fuchsia.googlesource.com/fuchsia/+/refs/heads/main/zircon/kernel/lib/userabi/userboot/main.rs)

userboot is deliberately minimal. Its only job is to bridge the gap between
the kernel's bootstrap channel and the first real program:

1. **Receives the bootstrap channel** via `take_system_handles()` and unpacks
   the 15 handles.

2. **Parses the ZBI** from the ZBI VMO (handle 4). Iterates ZBI items looking
   for `ZBI_TYPE_CMDLINE` entries to read boot options.

3. **Reads `userboot.next`** from the command line. Default:
   `bin/component_manager+--boot` (where `+` separates argv elements).

4. **Extracts bootfs** from the ZBI into a VMO. bootfs is a simple read-only
   filesystem image embedded in the ZBI.

5. **Loads the next program** from bootfs. Finds the ELF binary at the path
   specified by `userboot.next`, maps it into a new process, and starts it.

6. **Serves as a loader service** for the next program. The dynamic linker
   in the next process asks userboot to load shared libraries from bootfs.

7. **Exits** when the next program closes the loader service channel. This
   is why `component_manager` explicitly drops its loader service handle
   early in `main()`.

---

## What component_manager Does

Source: [`src/sys/component_manager/src/main.rs`](https://fuchsia.googlesource.com/fuchsia/+/refs/heads/main/src/sys/component_manager/src/main.rs)

component_manager is the first "real" Fuchsia program. It:

1. **Sets itself as job-critical** -- if it crashes, the system reboots.

2. **Closes the loader service** from userboot (allowing userboot to exit).

3. **Reads its runtime config** from bootfs.

4. **Initializes logging** (kernel log or syslog).

5. **Builds the component framework environment** -- registers ELF runners,
   service brokers, devfs, namespace resolvers.

6. **Starts the root component** -- the top of the Fuchsia component tree,
   which bootstraps all other services (drivers, networking, UI, etc.).

---

## How zCore Implements This

zCore's implementation lives in `loader/src/zircon.rs`. The main entry point
is `run_userstart()` (line 288), called from `primary_main()` in
`zCore/src/main.rs:60` via the backward-compatible alias `run_userboot()`.

### Current implementation

The current flow in `run_userstart()`:

1. **Creates kernel objects** (lines 289-298): root `Job`, `Process` named
   "userstart", `Thread` named "userstart", root `Resource`.

2. **Loads userstart ELF** (embedded at compile time via `include_bytes!`):
   parses the ELF headers with `xmas-elf`, maps PT_LOAD segments into the
   process VMAR with correct permissions.

3. **Creates stub vDSO** : a placeholder VMO named "vdso/full". Not a
   real vDSO -- userstart uses inline syscall wrappers from `zircon-abi`.

5. **Creates ZBI VMO** (lines 330-335): wraps the raw ZBI bytes in a VMO.

6. **Sets up the stack** (lines 338-349): 8 pages (32 KiB), mapped with
   `READ | WRITE | USER`.

7. **Creates bootstrap channel** (line 352): `Channel::create()` producing
   `(user_channel, kernel_channel)`.

8. **Packs 15 handles** (lines 355-393): matches the Fuchsia handle layout
   (see table above), though several are stubs.

9. **Writes the message** (line 393): cmdline with colons replaced by NUL
   bytes, plus the handle vector.

10. **Starts the process** (line 395): sets the thread entry point and stack
    pointer, passes `user_channel` as the start handle.

### Comparison with Fuchsia

| Aspect | Fuchsia | zCore (current) |
|--------|---------|-----------------|
| First process code | Embedded userboot ELF, loaded by kernel ELF parser | Flat binary from ZBI bootfs, or inline machine code |
| Program format | Full ELF with dynamic linking | Flat binary (via `objcopy -O binary`) |
| vDSO | Real shared library with syscall trampolines | Stub VMO (placeholder) |
| Syscall mechanism | Userspace calls vDSO functions | Userspace uses inline `svc`/`syscall` directly |
| Loader service | userboot serves as dynamic linker for next program | Not implemented |
| Next program | `bin/component_manager` from bootfs | petal `hello` program (or inline hello) |
| Bootstrap handles | All 15 are real, functional objects | Handles 0-4 are real; 5-14 are stubs |
| Command line | Parsed by userboot in userspace | Parsed by kernel, passed as message data |

### The userspace trap loop

After the process starts, `thread_fn()` (line 409) spawns the async
`run_user()` function (line 413) which handles the userspace execution loop:

1. Fires `ProcessStarting` and `ThreadStarting` exceptions.
2. Enters the main loop: `enter_uspace()` context-switches into userspace.
3. On trap back (syscall, interrupt, page fault), dispatches via
   `handler_user_trap()` (line 465):
   - **Syscalls**: extracts number and args from registers, calls
     `zircon_syscall::Syscall::syscall()`.
   - **Page faults**: handled by the VMAR.
   - **Interrupts**: forwarded to the HAL.
4. When the root process exits, calls `kernel_hal::cpu::reset()`.

---

## The Petal Test Userspace

petal programs serve as zCore's test userspace. They are minimal `#![no_std]`
Rust binaries that use `zircon-abi` inline syscall wrappers. See
[petal/README.md](../petal/README.md) for details.

The current petal program (`petal/src/bin/hello.rs`):

```rust
#[no_mangle]
pub extern "C" fn _start() -> ! {
    let msg = b"petal: Hello from petal on zCore!\n";
    unsafe {
        syscall::zx_debug_write(msg.as_ptr(), msg.len());
        syscall::zx_process_exit(0);
    }
}
```

Petal programs are linked at virtual address 0 via `petal/petal.ld`, then
converted to flat binaries with `objcopy`. The kernel maps the flat binary
at the start of the process VMAR.

---

## Gaps and Roadmap

To run real Fuchsia `userboot` (and subsequently `component_manager`) on
zCore, the following gaps must be addressed:

### 1. ELF Loader

**Gap:** zCore currently loads flat binaries only. Fuchsia's userboot is a
full ELF binary with program headers, relocations, and dynamic linking.

**Required:** An ELF loader in the kernel that can parse ELF headers and map
PT_LOAD segments into a process VMAR. The original Zircon kernel has this in
[`zircon/kernel/lib/userabi/userboot.cc`](https://fuchsia.googlesource.com/fuchsia/+/refs/heads/main/zircon/kernel/lib/userabi/userboot.cc).

### 2. Real vDSO

**Gap:** zCore provides a stub vDSO VMO. Fuchsia userspace programs expect
a real vDSO containing syscall trampoline functions. Without it, no standard
Fuchsia binary can make syscalls.

**Required:** Build or provide a vDSO shared library that exports the
`zx_*` syscall functions. The original is built from
[`zircon/kernel/lib/userabi/vdso/`](https://fuchsia.googlesource.com/fuchsia/+/refs/heads/main/zircon/kernel/lib/userabi/vdso/).

Alternatively, petal programs sidestep this by using inline syscall wrappers
from `zircon-abi`, which is fine for testing but not for running real Fuchsia
binaries.

### 3. Loader Service Protocol

**Gap:** Fuchsia's userboot serves as a loader service (dynamic linker) for
the next program. zCore does not implement this protocol.

**Required:** Implement the `fuchsia.ldsvc/Loader` FIDL protocol so that
dynamically linked programs can request shared libraries from bootfs. The
original implementation is in the userboot source directory.

### 4. Syscall Coverage

**Gap:** zCore implements a subset of the ~170 Zircon syscalls. Running
`component_manager` requires many more than `zx_debug_write` and
`zx_process_exit`.

**Required:** Incrementally implement syscalls as needed. The syscall
definitions are in
[`zircon/vdso/`](https://fuchsia.googlesource.com/fuchsia/+/refs/heads/main/zircon/vdso/)
(the FIDL/Banjo definitions that generate both vDSO and kernel stubs).

### 5. Real Bootstrap Handle Objects

**Gap:** Handles 5-14 in the bootstrap channel (vDSO variants, crashlog,
kcounters, instrumentation) are stubs in zCore.

**Required:** Provide real or at minimum correctly-sized VMOs so that
userboot can read expected data from them without faulting.

### Related Issues

- [#89](https://github.com/andrewdavidmackenzie/zCore/issues/89) -- Load
  petal programs from ZBI bootfs
- [#122](https://github.com/andrewdavidmackenzie/zCore/issues/122) -- Run
  real Fuchsia userspace binaries on zCore (ABI compatibility)
