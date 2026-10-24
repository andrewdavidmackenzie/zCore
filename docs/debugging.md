# Debugging zCore with GDB / LLDB

This guide covers remote debugging of zCore running in QEMU via GDB
or LLDB. QEMU's `-S -s` flags pause the VM at startup and open a
GDB server on `localhost:1234`.

## Quick Start

### x86_64 (GDB)

```bash
# Terminal 1: start QEMU paused with GDB server
make debug-x86-qemu          # Zircon shell
# or
make debug-x86-core-tests CORE_TESTS_FILTER='SuiteName.*'  # core-tests

# Terminal 2: connect GDB
make debug-x86-gdb
```

### aarch64 (LLDB)

```bash
# Terminal 1
make debug-qemu

# Terminal 2
make debug-lldb
```

## Kernel Symbols

The unstripped kernel ELF with full debug symbols is at:

| Arch    | Path                                    |
|---------|-----------------------------------------|
| x86_64  | `target/qemu-x86_64/release/kernel`     |
| aarch64 | `target/qemu-aarch64/release/kernel`    |

The `release` profile includes debug info (`debug = true` in
`Cargo.toml`), so symbols are available even in optimised builds.

## Common GDB Commands

```gdb
# Continue execution (QEMU starts paused)
(gdb) c

# Break on a kernel function
(gdb) b zircon_syscall::channel::Syscall::sys_channel_call_noretry

# Break on a module path
(gdb) b zircon_object::ipc::channel::Channel::push_general

# List threads (each QEMU vCPU is a GDB thread)
(gdb) info threads

# Switch to a specific CPU
(gdb) thread 7          # CPU 6 (GDB threads are 1-indexed)

# Backtrace
(gdb) bt

# Print a variable
(gdb) p self.base

# Continue to next breakpoint
(gdb) c

# Interrupt a running VM (like Ctrl-C in QEMU monitor)
# Just press Ctrl-C in the GDB terminal

# Step one instruction
(gdb) si

# Step one source line
(gdb) n
```

## Debugging a Hung Test

When a core-test hangs, use GDB to inspect what each CPU is doing:

```bash
# Terminal 1: start the test paused
make debug-x86-core-tests CORE_TESTS_FILTER='PortStressTest.*'

# Terminal 2: connect and let it run
make debug-x86-gdb
(gdb) c
# ... wait for hang, then Ctrl-C ...

# Check all CPUs
(gdb) thread apply all bt 5
```

Each QEMU vCPU maps to a GDB thread (thread 1 = CPU 0, thread 2 =
CPU 1, etc.). When the VM is interrupted, check which CPUs are in
`wait_for_interrupt` (HLT/idle), which are in `executor::run`, and
which are stuck in a specific function.

### Useful breakpoints for hang diagnosis

```gdb
# Break when a channel message is pushed
b zircon_object::ipc::channel::Channel::push_general

# Break on signal changes
b zircon_object::object::KObjectBase::signal_change

# Break on IPI send
b hal_impl::imp::arch::interrupt::send_ipi

# Break on executor idle
b executor::arch::x86_64::wait_for_interrupt

# Break on timer set
b hal_impl::imp::timer::timer_set
```

## RustRover / CLion

1. Build the kernel: `make core-tests-build`
2. Start QEMU: `make debug-x86-core-tests CORE_TESTS_FILTER='...'`
3. In RustRover: **Run > Attach to Process > Remote GDB Server**
   - Host: `localhost`, Port: `1234`
   - Symbol file: `target/qemu-x86_64/release/kernel`

## Notes

- **LTO**: release builds use LTO, which can inline or eliminate
  functions. If a breakpoint doesn't hit, the function may have been
  inlined. Try breaking on the caller instead.
- **`LOG=warn`**: info-level log messages are stripped by LTO in
  release builds. Use `error!()` for diagnostics that must survive,
  or `hal_impl::console::console_write_fmt` for raw output.
- **8 CPUs**: core-tests run with `ZCORE_MAX_CPUS=8`. Use
  `thread apply all bt` to see all CPUs.
- **OVMF**: x86_64 QEMU uses UEFI boot via OVMF. The `find-ovmf.sh`
  script locates the firmware automatically.
