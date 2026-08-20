# Porting zCore to RISC-V 64 (QEMU)

### 2021-03-04

Previously, memory used a simple approach of mapping
large pages via boot page tables. When loading the QEMU
filesystem image into memory and unpacking it, the
virtual memory, virtio-blk-device, and SimpleFileSystem
all need to be fully initialized.

Some components can conveniently use existing crate
libraries. The memory-related functions in the
intermediate abstraction layer `kernel-hal` that needed
implementation include:

* PageTable -- create or retrieve page tables
* trait PageTableTrait -- virtual-to-physical address
  mapping, queries, etc.
* hal_frame_alloc -- physical frame allocation
* pmem_write/read -- physical address access

The reusable page table implementation `PageTableImpl`
and memory area mapping `MemoryArea` from rCore were
ported into zCore. Functions and variables from the
bare-metal layer were exposed for use during boot, and
several bugs were fixed. Based on current runtime
results, the page table switching appears to work
correctly:

```
[1.2317591s DEBUG 0 0:0] switch table
    8000000000080220 -> 8000000000080a49
```

### zCore System Architecture
![](./structure.svg)

### RISC-V 64 Initial Porting Path

Following this path:

| QEMU riscv64 | - | kernel-hal-bare | - | kernel-hal | - | zircon-object / linux-object | - | linux-syscall | - | linux-loader | - | busybox |
|--------------|---|-----------------|---|------------|---|------------------------------|---|---------------|---|--------------|---|---------|

### Implementation Approach

* Analyze the Makefile structure to understand the
  compilation and execution command flow
* Adapt QEMU virt and OpenSBI for riscv64, including
  the kernel file format and user filesystem
  generation (from x86_64 to riscv64), and write
  these build commands into the Makefile
* Analyze the Cargo.toml structure to understand the
  relationships between features and dependency
  crates (some are optional)

* Create a Rust target-spec JSON file for riscv64
* Create the corresponding linker script (.ld) and
  boot entry assembly (.asm) for riscv64
* The main modifications are in kernel-hal-bare:
  add the riscv architecture under `arch/`, including
  hardware initialization and OpenSBI interface
  wrappers

* Set a small goal first: get the OS to boot and
  print some initial characters
* At this point there will be many compilation errors
  to resolve. Rust's error messages are very detailed
  and suggest fixes:
  - Errors may come from multiple sources: riscv64
    dependencies in Cargo.toml (disable non-essential
    crates first, add architecture-specific ones)
  - Missing interfaces in kernel-hal-bare that need
    implementation (see definitions in kernel-hal;
    reference the arch/x86_64 functions)
  - `target_arch` cfg changes from x86_64 to riscv64
    -- related functions and variables need to be
    added
  - Variable and function scope issues need attention
* Rust's conditional compilation `cfg`,
  `#[cfg(target_arch = "x86_64")]` also needs a
  riscv64 implementation

* To get OS printing working, initialize serial
  output:
  - Two approaches: call OpenSBI's print interface,
    or initialize serial output via MMIO
  - Then implement `fmt::Write` and the `println!`
    macro

* For interrupts, the `riscv` crate provides
  convenient instruction and register operations
* Fill in riscv64 context-switching assembly for trap
  handling
* Initialize S-mode interrupts including timer
  interrupts and PLIC external interrupts (different
  between QEMU and K210):
  - K210 reports an illegal instruction for `rdtime`
    and cannot get the instruction value via tval,
    so K210 cannot use
    `riscv::register::time::read()`. QEMU has no
    such issue.
  - Through joint OpenSBI debugging: when hardware
    triggers a timer interrupt, it sets the STIP bit
    in the `sip` register. After an instruction
    completes, if STIP is 1 and the STIE bit in
    `sie` is also 1, the S-mode timer interrupt
    handler is entered.
  - When M-mode does not delegate timer interrupts
    to S-mode, QEMU's M-mode can receive S-mode
    timer interrupts.
  - K210's M-mode cannot receive S-mode interrupts.
    Timer and software interrupts can be delegated
    to S-mode, but PLIC external interrupts cannot
    be delegated even when configured to do so.

* PLIC external interrupts are critical for UART
  serial output and virtio-blk-device filesystem
  loading:
  - QEMU UART0_IRQ=10, K210 serial ID=33
  - Configure the PLIC registers via MMIO: set
    interrupt source priority (0-7, 7 highest), set
    the target global threshold [0..7] (interrupts
    <= threshold are masked)
  - Enable interrupts for a given ID on a target.
    Interrupt IDs can be found in
    `qemu/include/hw/riscv/virt.h`. Note that
    different privilege modes on the same hart are
    different targets, with a stride of 0x80:

| Target: | 0 | 1 | 2 |        | 3 | 4 | 5 |
|---------|---|---|---|--------|---|---|---|
| Hart0:  | M | S | U | Hart1: | M | S | U |

Running on Hart0 S-mode after OpenSBI means Target 1.

* After PLIC interrupt initialization, initialize
  UART interrupts. QEMU virt UART base address is
  `0x1000_0000`, K210 is `0x38000000`.
  - The UART interrupt handler prints each character
    as it's typed.

* Next: handle virtual memory and the filesystem.

* QEMU boots OpenSBI, loads the kernel, and jumps
  to `_start`. Initialize logging, physical memory,
  then enter hardware initialization.
* Load the ramfs filesystem as a slice into memory at
  a specified address, open the SimpleFileSystem, and
  execute busybox via linux_loader.

* Parse the Simple FileSystem generated by
  rcore-fs-fuse. Use `SimpleFileSystem::open()` to
  open the in-memory filesystem and read files and
  directories.
* Finally: `linux_loader::run busybox sh`

* Previously, the rboot UEFI bootloader placed the
  initramfs at a specified memory address.
* Memory must be properly initialized. QEMU's virtio
  block device is used here and also needs
  initialization.

Porting is not yet complete...

The system running demo is shown above.

Many thanks to the teachers and fellow students for
their help during the porting process!
