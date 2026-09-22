# Crate Dependency Tree

This document shows the dependency relationships between all workspace crates
in the zCore project.

For crate descriptions, see [architecture.md](architecture.md).
For external (non-workspace) dependencies, see [external-
dependencies.md](external-dependencies.md).

## Dependency Tree

### Full Crate Dependency Tree

Each crate is shown with its direct workspace dependencies indented below it.
Leaf crates (no workspace dependencies) are at the top; the final binary is at
the bottom.

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
hal-impl/
 +-- drivers/
 |   (always, feature "virtio"; optional
 |    features: mock, graphic, loopback)
 +-- third-party/executor/
     (bare-metal only, target_os = "none")

hal-impl uses driver scheme traits (BlockScheme,
UartScheme, etc.) to manage device registries and
provides the FFI boundary functions
(virtio_dma_alloc, drivers_phys_to_virt) that
drivers call. The HAL is the integration layer:
it initializes drivers, registers them in
DeviceList<T>, and exposes `all_block()`,
`all_uart()` etc. to the rest of the kernel.


zCore/zircon-object/
 +-- hal-impl/
 +-- third-party/region-alloc/

Confirmed. `zircon-object` uses `region-alloc`
only for PCI BAR allocation, not for general
memory. The main frame and heap allocators are
in the `zCore` binary crate (`memory.rs` /
`memory_x86_64.rs`). The HAL accesses them via
`KernelHandler` callbacks. All allocation runs
in kernel (supervisor) mode.


LEVEL 2 (depends on Level 1 + leaf crates)
========================================
linux-object/
 +-- zircon-object/    (with feature "elf")
 +-- hal-impl/       (default-features=false)
 +-- drivers/          (with feature "virtio")
 
linux-object uses `drivers::get_sockets()` to
access the global smoltcp socket set for TCP/UDP
networking, and uses `BlockScheme`, `UartScheme`,
`DisplayScheme` for device filesystem nodes
(/dev/ttySN, /dev/fb0, block devices). It needs
direct driver access for Linux device emulation.

 

zCore/zircon-syscall/
 +-- zCore/zircon-object/
 +-- hal-impl/       (default-features=false)


LEVEL 3 (depends on Level 2)
========================================
linux-syscall/
 +-- zircon-object/
 +-- linux-object/
 +-- hal-impl/       (default-features=false)

TODO Describe why linux-syscall also depends on zircon-object
and not linux-object only
  > See the earlier linux-syscall/zircon-object TODO.
  > linux-syscall uses Process, Thread, VmObject,
  > MMUFlags, Signal, ThreadState directly from
  > zircon-object. These are the shared kernel types.
  > Ideally linux-object would re-export them, but
  > currently linux-syscall reaches through both.
 

LEVEL 4 (integration hub)
========================================
loader/
 +-- hal-impl/
 +-- zircon-object/
 +-- linux-object/     (optional, "linux")
 +-- linux-syscall/    (optional, "linux")
 +-- zircon-syscall/   (always included)
 +-- third-party/executor/

The loader runs in **kernel (supervisor) mode**.
It directly creates kernel objects (Job, Process,
Thread, VMAR), loads ELF segments into VMOs,
maps them into the process address space, then
calls `ctx.enter_uspace()` to switch INTO user
mode. The `run_user()` async loop handles traps
and dispatches syscalls -- all in supervisor mode.


The loader crate supports BOTH: `pub mod linux`
and `pub mod zircon` are independently feature-
gated. Both features can be enabled simultaneously
(in fact, `loader/Cargo.toml` defaults to both
on). However, at RUNTIME only one personality is
used per boot -- the zCore binary selects which
`run()` to call based on its own feature flags.


The loader crate CAN compile both `linux` and `zircon`
modules simultaneously (its Cargo.toml defaults both on).
However, the `zCore` binary crate's `main.rs` enforces
mutual exclusivity at compile time: if both `linux` and
`zircon` features are enabled, it panics. So the loader
is capable of both, but only one is used per kernel
build.

LEVEL 5 (final kernel binary)
========================================
zCore/ (binary)
 +-- hal-impl/
 +-- zircon-object/
 +-- loader/
 +-- third-party/executor/
 +-- linux-object/     (optional, "linux")

Split across both, with distinct roles:
**zircon-object**: core kernel LOGIC -- process/
thread/job model, virtual memory (VMAR, VMO),
IPC (channels, sockets, fifos, ports), signals,
futexes, resource management. ~15K lines.
**hal-impl**: hardware INTERACTION -- page table
management, interrupt handling, timer, context
switching, DMA, driver registration. ~8K lines.
**Scheduling** is implicit via the async executor.
Neither crate alone is "the kernel" -- together
they form it.


Combining hal-impl and zircon-object would
create a circular dependency problem: hal-impl
depends on `drivers` (for device management), and
zircon-object depends on hal-impl (for page
tables). If merged, drivers would need to depend
on the combined crate, but the combined crate
depends on drivers -> circular. The current split
is driven by Rust's acyclic crate graph. A trait-
based approach (HAL trait crate + impl crates)
would be cleaner but still requires separate
crates. Covered by the HAL cleanup issue.


Major divergences from a clean HAL model:
(1) **Drivers are platform-independent** -- yes,
they use abstract bus I/O (volatile read/write)
and the HAL provides the platform-specific glue
(phys_to_virt, DMA alloc) via FFI. This part is
clean.
(2) **hal-impl mixes interface and impl** --
`hal_fn.rs` defines the interface, `bare/` and
`libos/` provide implementations, but `common/`
has shared types AND logic (futures, user pointer
validation, PhysFrame RAII). The shared logic
should arguably be in the kernel, not the HAL.
(3) **zircon-object has core kernel functionality**
-- yes, it includes process scheduling
(thread blocking/waking), memory management
(VMAR map/unmap/protect), and IPC (channel
message passing). These ARE kernel functions,
not just "Zircon objects". The name is misleading.
See [#96](https://github.com/andrewdavidmackenzie/zCore/issues/96).

 

BUILD TOOL (not linked into the kernel)
========================================
xtask/
 +-- z-config/
```

### Visual Dependency Graph

Arrows point from dependant -> dependency ("A --> B" means A depends on B).

```
       BUILD-TIME ONLY
      +------------------------------+
      |  Cargo.toml                   |
      |  [workspace.metadata.machines]|
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
  |  |     hal-impl       |<---+      |
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
| `hal-impl`    | `drivers`      | Uses driver   |
|                 |                | scheme traits |
|                 | `executor`     | Bare-metal    |
|                 |                | thread spawn  |
| `zircon-object` | `hal-impl`   | UserContext,  |
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
|                  | `hal-impl`    | Timer,       |
|                  |                 | console,     |
|                  |                 | user ptrs    |
|                  | `drivers`       | Socket set,  |
|                  |                 | block/UART   |
| `zircon-syscall` | `zircon-object` | All Zircon   |
|                  |                 | kernel objs  |
|                  | `hal-impl`    | User ptrs,   |
|                  |                 | timers       |

#### Level 3

| Crate           | Dep             | Relationship  |
|-----------------|-----------------|---------------|
| `linux-syscall` | `zircon-object` | Process, VMO, |
|                 |                 | Thread        |
|                 | `linux-object`  | All Linux     |
|                 |                 | abstractions  |
|                 | `hal-impl`    | User ptrs,    |
|                 |                 | timers        |

#### Level 4

| Crate    | Dep              | Relationship       |
|----------|------------------|---------------------|
| `loader` | `hal-impl`     | User context, VM    |
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
| `zCore`  | `hal-impl`    | HAL init, config    |
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
drivers -> hal-impl -> zircon-object
  -> linux-object -> linux-syscall
  -> loader -> zCore
```

A change in `drivers` can transitively affect every crate in the project.

### Feature-Gated Dependencies

| From         | To               | Feature Gate       |
|--------------|------------------|--------------------|
| `loader`     | `linux-object`   | `linux` feature    |
| `loader`     | `linux-syscall`  | `linux` feature    |
| `loader`     | `zircon-syscall` | `zircon` feature   |
| `zCore`      | `linux-object`   | `linux` feature    |
| `hal-impl` | `executor`       | bare-metal only    |
|              |                  | (target_os="none") |
| `zircon`     | `xmas-elf`       | `elf` feature      |
| `-object`    |                  |                    |
| `zircon`     | hypervisor mod   | `hypervisor`       |
| `-object`    |                  | (currently off)    |
| `zircon`     | hypervisor mod   | `hypervisor`       |
| `-syscall`   |                  | (currently off)    |

Loader DOES depend on zircon-object (for Process, Thread, Job, VMAR, Channel,
Handle, Rights, ELF loader). It ALSO depends on linux-object (optional, behind
`linux` feature). Both are listed in loader/Cargo.toml. The difference: zircon-
object is always needed (both personalities share it); linux-object is only
needed for Linux mode.

Feature combination testing
(see [#80](https://github.com/andrewdavidmackenzie/zCore/issues/80)):
a CI job should test: (1) `--features linux` only,
(2) zircon-only (no extra features), (3) `--features
linux,libos`, (4) zircon + libos,
(5) bare-metal aarch64/riscv64/x86_64. Note: `linux`
+ `zircon` together will panic at runtime but should
compile. Could use `cargo-hack` for systematic
feature testing.

---

## External Dependencies
