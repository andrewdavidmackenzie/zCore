# External Dependencies

External (non-workspace) crate dependencies used across the zCore project,
classified by role.

For workspace crate dependencies, see [dependency-tree.md](dependency-tree.md).

## External Dependencies

External (non-workspace) crate dependencies used across the project, classified
by role.

### Major Subsystem Libraries

These are large, complex crates that provide significant functionality to
zCore. A change or update in any of these would have broad impact.

**`smoltcp`** (git, rev `35e833e3`) User-space TCP/IP network stack. Provides
TCP, UDP, raw, and ICMP sockets. Used by `drivers`, `kernel-hal`, and `linux-
object`. Pinned to a specific git revision. Features enabled: `proto-ipv4`,
`proto-ipv6`, `proto-igmp`, `socket-raw`, `socket-udp`, `socket-tcp`, `socket-
icmp`, `async`.

Latest smoltcp release is 0.11.0 (2024). The pinned revision predates it.
Upgrading requires checking API compatibility (smoltcp's API changes
between versions). See [#97](https://github.com/andrewdavidmackenzie/zCore/issues/97).


**`rcore-fs` family** (git, rev `1a3246b`) Virtual filesystem framework from
the rCore project. Seven crates from one repo:
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

The FS API is `rcore-fs`'s `FileSystem` and `INode` traits (open, read, write,
stat, lookup, readdir). Any FS implementing these traits works. Options: (1)
ext2/ext4 (via rcore-fs-ext2, not currently included), (2) FAT32 (common for
embedded), (3) a Rust-native FS like `fatfs` or `ext4-rs`. For Zircon mode, the
real Fuchsia approach is userspace FS servers over channels -- no kernel FS API
needed.
See [#76](https://github.com/andrewdavidmackenzie/zCore/issues/76).


**`trapframe`** (0.9.0, crates.io) User/kernel context save/restore and trap
frame structures. Provides `UserContext` for entering and returning from user-
space. Used by `kernel-hal`. Critical for the entire syscall entry/exit path.

**`virtio-drivers`** (git, rev `2aaf7d6`) VirtIO device driver implementations
(block, GPU, input, console, network). Used by `drivers` behind the `virtio`
feature (enabled by default).

**`futures`** (0.3, crates.io) Async primitives (`oneshot`, `select_biased!`,
`FutureExt`). Used with `no_std` + `alloc` + `async-await` features. Used by
`zircon-object`, `zircon-syscall`, `linux-syscall`.

**`async-std`** (1.10, crates.io) Full async runtime for the host OS. Provides
task spawning, sleeping, I/O, and the `#[async_std::main]` /
`#[async_std::test]` macros. Used in libos mode by `kernel-hal`, `zCore`,
`drivers`; used in tests by `zircon-object`, `linux-syscall`, `loader`.

### Architecture-Specific Hardware Crates

These crates provide register access, instruction wrappers, and hardware
abstractions for specific CPU architectures.

**AArch64:**
- `cortex-a` (7.2.0) -- ARM Cortex-A register access. Used by `kernel-hal`,
  `executor`.
- `tock-registers` (0.7) -- Type-safe MMIO register definitions. Used by
  `kernel-hal`, `executor`.

**RISC-V:**
- `riscv` (0.8/0.9) -- CSR access, `satp` register. Used by `drivers`, `kernel-
  hal`, `zCore`, `executor`. Note: two different versions in the workspace.
- `sbi-rt` (0.0.2) -- SBI runtime calls (hart start, system reset). Used by
  `kernel-hal`, `zCore`.
- `dtb-walker` (0.2.0-alpha.3) -- Device tree blob parsing. Used by `zCore`
  (riscv only).
- `page-table` (0.0.6) -- Sv39 page table types. Used by `zCore` (riscv only).
- `r0` (1) -- BSS zeroing utility. Used by `zCore` (riscv only).

**x86_64:**
- `x86_64` (0.14) -- Page tables, GDT, IDT, control registers. Used by
  `drivers`, `kernel-hal`, `executor`.
- `x86` (0.46) -- I/O ports, MSRs, segment registers. Used by `kernel-hal`.
- `x2apic` (0.4) -- Local APIC and I/O APIC drivers. Used by `drivers`,
  `kernel-hal`.
- `raw-cpuid` (9.0/10.2) -- CPUID instruction wrapper. Used by `kernel-hal`,
  `executor`. Two versions.
- `uefi` (0.16) -- UEFI boot services types. Used by `kernel-hal` (bare-metal
  x86_64).
- `rboot` (git, rev `ad21575`) -- UEFI bootloader interface (`BootInfo`
  struct). Used by `zCore` (bare-metal x86_64).
- `x86-smpboot` (git, rev `1069df3`) -- SMP AP startup. Used by `kernel-hal`
  (bare-metal x86_64).
- `acpi` (4.1) -- ACPI table parsing. Used by `drivers` (x86_64).

### Device and Driver Support Crates

**`device_tree`** (git, rev `2f2e55f`) Flattened device tree (FDT/DTB) parser.
Used by `drivers` for device discovery on aarch64/riscv64.

**`isomorphic_drivers`** (git, rev `f7cd97a8`) Platform-independent driver
implementations from the rCore ecosystem. Used by `drivers`.

**`pci`** (git, rev `8f33774b`) PCI bus scanning and configuration space
access. Used by `drivers`.

**`bitmap-allocator`** (git, rev `88e871a5`) Bitmap-based physical frame
allocator. Used by `drivers`, `kernel-hal` (libos), `zCore` (x86_64 + libos).

**`d1-pac`** (0.0.27, optional) Allwinner D1 peripheral access crate. Used by
`drivers` behind `allwinner` feature.

### Synchronization and Concurrency

**`lock`** (git, kernel-sync, rev `8486b8`) Kernel-compatible `Mutex` /
`RwLock`. The most widely used sync primitive across the project. Used by 7
crates: `drivers`, `kernel-hal`, `zircon-object`, `zircon-syscall`, `linux-
object`, `linux-syscall`, `zCore`.

**`spin`** (0.9) `Once<T>` for one-time initialization and spinlocks. Used by
`kernel-hal`, `zCore`, `executor`.

**`lazy_static`** (1.4) Lazy-initialized statics. Used with `spin_no_std`
feature in kernel crates. Used by `drivers`, `kernel-hal`, `zircon-object`,
`linux-object`, `linux-syscall`, `executor`.

`std::sync::LazyLock` is std-only, not no_std. `core::cell::LazyCell` (Rust
1.80) is NOT Sync, so it can't be used in statics. The correct no_std
replacement is `spin::Lazy` (from the `spin` crate, already a dependency). This
would eliminate the `lazy_static` dep entirely. See earlier issue candidate for
this.


### Utility and Glue Crates

**`log`** (0.4) Logging facade. The single most widely used dependency
  -- present in 9 of 12 workspace crates.

**`cfg-if`** (1.0) Conditional compilation macros. Used by 8 crates.

**`bitflags`** (1.3) Typed bitflag definitions (`Signal`, `Rights`, `MMUFlags`,
`VmarFlags`, etc.). Used by 6 crates.

**`numeric-enum-macro`** (0.2) Derives numeric enums with `TryFrom`. Used for
`SyscallType`, `ResourceKind`, `FcntlCmd`, etc. Used by 6 crates.

**`hashbrown`** (0.9) `no_std`-compatible hash map. Used by `zircon-object`
(handle storage) and `linux-object` (fd table, futex table).

**`downcast-rs`** (1.2) Runtime downcasting for trait objects. Used by `zircon-
object` (`Arc<dyn KernelObject>` -> concrete type) and `linux-object` (`dyn
FileLike` -> `EpollFile`).

**`xmas-elf`** (0.7) ELF binary parser. Used by `zircon-object` (behind `elf`
feature), `linux-object`, and `loader` (behind `zircon` feature).

**`bitvec`** (0.22) Bit-vector type for `FdSet` in select/pselect. Used by
`linux-syscall`.

**`static_assertions`** (1.1.0) Compile-time struct size checks. Used by
`linux-syscall`.

**`async-trait`** (0.1) Async methods in traits. Used by `linux-object`.

### Allocators

**`customizable-buddy`** (0.0.3) Buddy allocator for heap memory (up to 64
GiB). Used by `zCore` for the global heap on non-x86 platforms.

**`buddy_system_allocator`** (0.8) Alternative heap allocator. Used by `zCore`
on bare-metal x86_64 only.

### LibOS-Only Dependencies

These are used exclusively when running in library OS mode on a host system.

**`nix`** (0.23) Unix API bindings (mmap, mprotect, signal). Used by `kernel-
hal` for the mock memory subsystem.

"Mock" drivers in `drivers/src/mock/` are software-only implementations of
hardware device interfaces used in libos mode:
- `MockUart`: reads from host stdin, writes to host stderr (simulates a serial
  port)
- `MockDisplay`: heap-allocated framebuffer Vec
- `MockKeyboard`/`MockMouse`: event-driven stubs They implement the same Scheme
  traits as real drivers, so the kernel code is identical in both modes. `nix`
  provides the mmap/mprotect calls that the mock memory subsystem uses to
  simulate physical RAM via a temp file.


Note: "mock drivers" could also be called "host OS drivers" since they delegate
to real host OS facilities (stdin, stderr, mmap) rather than truly mocking
hardware behavior.

**`tempfile`** (3) Temporary file creation. Used by `kernel-hal` for the mock
physical memory backing store.

**`chrono`** (0.4) Date/time formatting. Used by `zCore` for human-readable log
timestamps in libos mode.

**`sdl2`** (0.34) SDL2 bindings for graphics window. Used by `drivers` behind
`mock` feature for the mock display.

### Executor-Internal Dependencies

**`unicycle`** (git, personal fork) Async stream utilities. Used internally by
`executor`.

**`woke`** (0.0.2) Waker utilities. Used internally by `executor`.

**`bit-iter`** (1.0.0) Bit iterator. Used internally by `executor`.

### Build Tool Dependencies (xtask only)

These are used only by `xtask` and never linked into the kernel.

**`clap`** (4.0) -- CLI argument parsing with derive macros. **`os-xtask-
utils`** (0.0.0) -- Wrappers around cargo, qemu, git, make, binutils commands.
**`shadow-rs`** (0.11) -- Build metadata embedding (git rev, timestamps). Also
a build-dependency. **`dircpy`** (0.3) -- Directory copying. **`once_cell`**
(1.15) -- Lazy initialization. **`rand`** (0.8) -- Random number generation.
**`num_cpus`** (1) -- CPU count detection.

### Configuration Dependencies (z-config only)

**`serde`** (1.0) + **`serde_derive`** (1.0) -- Serialization framework for
TOML parsing. **`toml`** (0.5.9) -- TOML file parser.

### Console and Graphics Support

**`rcore-console`** (git, rev `ca5b1bc`) Text console rendering on a
framebuffer. Used by `drivers` behind `graphic` feature.

**`volatile`** (0.3) Volatile memory access for MMIO registers. Used by
`drivers`.

### Dependency Source Summary

| Source      | Count | Examples                |
|-------------|-------|-------------------------|
| crates.io   | 41    | log, bitflags, futures, |
|             |       | spin, clap, xmas-elf    |
| Git (rcore  | 9     | rcore-fs-*, virtio-     |
| ecosystem)  |       | drivers, device_tree,   |
|             |       | bitmap-allocator        |
| Git (other) | 5     | smoltcp, lock, pci,     |
|             |       | unicycle, x86-smpboot   |

Reducing git dependencies is tracked in
[#98](https://github.com/andrewdavidmackenzie/zCore/issues/98).
Priority targets: `smoltcp` (has 0.11 release),
`lock`/kernel-sync (could fork/publish), `virtio-
drivers` (has releases), `rcore-fs` family
(significant effort). The `rcore-os/*` ecosystem
crates are research-quality code not published to
crates.io.


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

Will create `docs/libos.md` and add link from README.md when libos mode is
fixed (#80).


## LibOS Mode
