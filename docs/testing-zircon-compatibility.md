# Testing Zircon Binary Compatibility

This document describes how to run Fuchsia's `core-tests-standalone` test suite
on zCore to verify Zircon binary compatibility.

## Overview

`core-tests-standalone` is Fuchsia's Zircon kernel test suite, compiled as a
standalone ELF binary that runs directly from userboot. It tests syscalls,
memory management, IPC, threading, and other kernel features. Running it on
zCore validates that our Zircon kernel implementation is binary-compatible
with stock Fuchsia userspace.

## Prerequisites

### Prebuilt binaries

The following Fuchsia binaries are needed in `prebuilt/zircon/x86_64/`:

| Binary | Purpose |
|--------|---------|
| `core-tests-standalone` | The test binary (PIE, dynamically linked) |
| `ld.so.1` | Fuchsia's dynamic linker (ld.so.1 IS libc.so) |
| `libc.so` | C library (same binary as ld.so.1) |
| `libc++.so.2` | C++ standard library |
| `libc++abi.so.1` | C++ ABI support |
| `libunwind.so.1` | Stack unwinding |
| `libtrace-engine.so` | Tracing support |
| `libinspector.so` | Process inspection |
| `libmini-process.so` | Mini-process for testing |
| `libzircon.so` | vDSO symbol table (not used directly) |

All binaries must be from the **same Fuchsia build** to ensure ABI compatibility.
They are copied to the rootfs at `/bin/core-tests-standalone` and `/lib/`.

### Building

```bash
cargo qemu --arch x86_64
# Or specifically for testing:
ZCORE_CMDLINE="LOG=warn ROOTPROC=/bin/core-tests-standalone" cargo bin -m qemu-x86_64
```

## Boot sequence

1. **Kernel boots**, loads the SFS rootfs image
2. **`spawn_process`** loads `core-tests-standalone`, detects `PT_INTERP` → loads `ld.so.1`
3. **Kernel sends two processargs messages** on the bootstrap channel:
   - Message 1 (for ld.so.1): PA_PROC_SELF, PA_THREAD_SELF, PA_VMAR_ROOT, PA_JOB_DEFAULT,
     PA_VMO_VDSO, PA_RESOURCE, PA_VMAR_LOADED, PA_VMO_EXECUTABLE, PA_LDSVC_LOADER
   - Message 2 (for libc): same essential handles plus PA_MMIO_RESOURCE, PA_IRQ_RESOURCE,
     PA_SYSTEM_RESOURCE, and argv
4. **ld.so.1** reads message 1, self-relocates (RELR), resolves PLT, loads shared libraries
   via the FIDL loader service
5. **libc startup** (`StartCompilerAbi`) reads message 2 via `_zx_startup_get_handles`,
   sets up TLS, stacks, thread state
6. **`__libc_extensions_init`** processes remaining handles (resources, etc.)
7. **Scudo allocator** initializes (maps 11.5 GB lazy arena via ALLOW_FAULTS)
8. **`main()`** calls `standalone::GetOptions()` (reads ZBI VMO for cmdline),
   then `RUN_ALL_TESTS()`

## How the vDSO works

The vDSO (`libzircon.so`) is generated in pure Rust by `zircon-loader/build.rs`.
It is a minimal ELF shared library containing:

- **Syscall trampolines** for all 231 syscalls: `mov %rcx,%r10; mov $NUM,%eax; syscall; ret`
  (the `mov %rcx,%r10` is needed because the C calling convention puts arg4 in `rcx`,
  but `syscall` clobbers `rcx`)
- **Wrapper functions** for userspace-only APIs like `clock_get_monotonic`,
  `system_get_page_size`, `thread_self`, `channel_call`, `cprng_draw`, etc.
- **ELF metadata**: `.dynsym`, `.hash`, `.dynamic` sections so ld.so.1 can
  resolve symbols by name

The kernel eagerly resolves 70+ `_zx_*`/`zx_*` PLT entries in ld.so.1 before
starting the process, because ld.so.1's lazy binding resolver isn't initialized
at that point.

## Processargs wire format

The processargs protocol is NOT FIDL. It uses a direct C struct wire format:

```
[zx_proc_args_t header (36 bytes)]
[uint32_t handle_info[N]]
[NUL-separated argv strings]
```

The `zx_proc_args_t` struct:
```c
struct zx_proc_args {
    uint32_t protocol;       // 0x4150585d
    uint32_t version;        // 0x00001000
    uint32_t handle_info_off;
    uint32_t args_off;
    uint32_t args_num;
    uint32_t environ_off;
    uint32_t environ_num;
    uint32_t names_off;
    uint32_t names_num;
};
```

Handle info entries encode type and argument: `PA_HND(type, arg) = (type & 0xFF) | ((arg & 0xFFFF) << 16)`

## Loader service (FIDL)

The loader service uses FIDL-style messages with 64-bit ordinals:

| Ordinal | Operation |
|---------|-----------|
| `0x48C5A151D6DF2853` | LOAD_OBJECT (load a shared library by name) |
| `0x6A8A1A1464632841` | CONFIG (set search path prefix) |
| `0x63BA6B76D3671001` | DONE (loading complete) |

Request format: `fidl_message_header_t` (16 bytes) + `fidl_string_t` (size + pointer) + string data.
Response format: `fidl_message_header_t` + `zx_status_t` + handle marker, with VMO handle in the handles array.

## VMAR lazy mappings

Scudo (Fuchsia's memory allocator) requires lazy-commit VMAR mappings:

- Maps ~11.5 GB with `ZX_VM_ALLOW_FAULTS` and a zero-length VMO named `scudo:reserved`
- Pages are committed on demand via page faults
- The page fault handler checks the mapping's `permissions` field (not per-page flags)
  to determine access rights
- Per-page flags are deferred for mappings >256 MB to avoid exhausting kernel heap
- `ZX_VM_SPECIFIC_OVERWRITE` allows mapping over existing mappings within the arena

## Test configuration

The test framework reads configuration from a ZBI VMO (`PA_VMO_BOOTDATA`):
- `--gtest_filter=<pattern>` — filter which tests to run
- `--gtest_repeat=<N>` — repeat tests N times

Without a ZBI VMO, all tests run with default settings.

## Current status

As of PR #467:
- The test binary loads and runs without crashes
- All 269+ syscalls succeed
- Prints `*** Running standalone test directly from userboot ***`
- Scudo allocator initializes successfully
- Process hangs after setup — investigating whether tests are executing
  or stuck on a missing syscall/feature

## Debugging notes

- **LTO strips `log` macros** in release builds. Use `hal_impl::console::console_write_str/fmt`
  for diagnostics in `zircon-syscall` and `zircon-object` crates.
- **Incremental builds can be stale.** Touch source files and verify with
  `strings target/.../kernel | grep "your string"`.
- **Fuchsia source** available at `amackenz@192.168.1.126:~/RustroverProjects/fuchsia/fuchsia/fuchsia/`
- The test binary uses the `zxtest` framework (Google Test compatible).
  Test output uses `[==========]`, `[  RUN  ]`, `[  PASSED  ]` markers.

## References

- Fuchsia processargs: `zircon/system/public/zircon/processargs.h`
- Fuchsia startup protocol: `sdk/lib/c/include/zircon/startup.h`
- ld.so.1 source: `zircon/third_party/ulib/musl/ldso/dynlink.c`
- libc startup: `sdk/lib/c/startup/start-compiler-abi.cc`, `processargs-get-handles.cc`
- Standalone test init: `src/zircon/testing/standalone-test/standalone-init.cc`
- Test harness: `src/zircon/testing/standalone-test/zxtest-main.cc`
- Scudo on Fuchsia: `third_party/scudo/src/mem_map_fuchsia.cpp`

## Known issues being debugged

### Page fault writing to executable rx segment (0x6de1e0)

After Scudo initializes and the test framework starts, a page fault occurs
writing to `0x6de1e0`. The address appears to be in the executable's rx
(read-execute) segment. The write is from libc code (rip in ld.so.1).

This may be caused by:
1. Scudo's `SPECIFIC_OVERWRITE` mapping not properly replacing the old
   mapping's page table permissions
2. The `VmMapping::cut` split creating a fragment with wrong permissions
3. An address space layout issue from `USER_ASPACE_BASE = 0x200000`

The backtrace shows:
```
rip=0x3bcbd2 (libc offset 0xbcbd2)
ret=0x6241d9 (executable)
ret=0x57b552 (executable)
ret=0x57aa15 (executable)
```

### What the test framework needs

The standalone test framework (`zxtest-main.cc`) requires:
- [x] `PA_RESOURCE` (0x3F) — root resource handle
- [x] `PA_SYSTEM_RESOURCE` (0x54) — system resource handle
- [x] `PA_MMIO_RESOURCE` (0x50) — MMIO resource handle
- [x] `PA_IRQ_RESOURCE` (0x51) — IRQ resource handle
- [x] `PA_VMO_BOOTDATA` — ZBI VMO named "zbi" (for kernel cmdline / test options)
- [ ] `PA_VMO_BOOTDATA` — VMO named "boot-options.txt" (for boot options)
- [ ] Working `RESOURCE_CREATE` for creating sub-resources
- [ ] Working Scudo allocator (VMAR_MAP with SPECIFIC_OVERWRITE + ALLOW_FAULTS)
