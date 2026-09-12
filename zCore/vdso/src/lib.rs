//! Zircon vDSO implementation.
//!
//! This crate provides the syscall trampolines and system constant
//! accessors that form the vDSO (`libzircon.so`). The vDSO is mapped
//! into every Zircon process by the kernel.
//!
//! The vDSO contains:
//! - Assembly trampolines for all ~158 syscalls
//! - Userspace-only functions that read from a kernel-mapped data
//!   page (VdsoConstants) without making syscalls
//!
//! The VdsoConstants data page is mapped by the kernel at a known
//! offset in the vDSO VMO. Functions like `zx_system_get_num_cpus`
//! and `zx_ticks_per_second` read directly from this page.

#![no_std]
#![deny(warnings)]

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {
        core::hint::spin_loop();
    }
}

// Include the generated assembly trampolines.
// The build script generates arch-specific assembly for all syscalls.
include!(concat!(env!("OUT_DIR"), "/trampolines.rs"));

// ── Userspace wrapper functions ─────────────────────────────────────
//
// These functions read directly from the kernel-mapped VdsoConstants
// data page, avoiding syscall overhead. The data page address is
// resolved at link time via the `VDSO_DATA` symbol that the linker
// script places at offset 0x7000.
//
// For now, these are provided as Rust functions. In the final vDSO
// ELF, they would override the trampoline stubs for the corresponding
// syscall numbers.

/// VdsoConstants layout matching kernel-hal/src/common/vdso.rs.
#[repr(C)]
pub struct VdsoConstants {
    max_num_cpus: u32,
    features_cpu: u32,
    hw_breakpoint_count: u32,
    hw_watchpoint_count: u32,
    dcache_line_size: u32,
    icache_line_size: u32,
    ticks_per_second: u64,
    ticks_to_mono_numerator: u32,
    ticks_to_mono_denominator: u32,
    physmem: u64,
    version_string_len: u64,
    version_string: [u8; 64],
}

// The data page pointer. In the final vDSO ELF this would be resolved
// by the linker to the data section at offset 0x7000. For now it's a
// null pointer that must be patched by the kernel before use.
//
// TODO: Replace with a linker-resolved symbol once the vDSO is built
// as a proper ELF shared library.
static mut VDSO_DATA_PTR: *const VdsoConstants = core::ptr::null();

/// Set the VdsoConstants data page pointer.
///
/// # Safety
/// Must be called exactly once before any data-page accessor is used.
/// The pointer must be valid for the lifetime of the process.
#[no_mangle]
pub unsafe extern "C" fn _vdso_set_data_ptr(ptr: *const VdsoConstants) {
    unsafe {
        VDSO_DATA_PTR = ptr;
    }
}

/// Read VdsoConstants safely, returning None if the pointer hasn't been set.
fn data() -> Option<&'static VdsoConstants> {
    let ptr = unsafe { VDSO_DATA_PTR };
    if ptr.is_null() {
        None
    } else {
        Some(unsafe { &*ptr })
    }
}

/// Get the number of CPUs. Reads from the vDSO data page.
#[no_mangle]
pub extern "C" fn zx_system_get_num_cpus() -> u32 {
    data().map_or(0, |d| d.max_num_cpus)
}

/// Get the system page size. Always 4096 on all Zircon platforms.
#[no_mangle]
pub extern "C" fn zx_system_get_page_size() -> u32 {
    4096
}

/// Get the amount of physical memory in the system.
#[no_mangle]
pub extern "C" fn zx_system_get_physmem() -> u64 {
    data().map_or(0, |d| d.physmem)
}

/// Get the number of ticks per second.
#[no_mangle]
pub extern "C" fn zx_ticks_per_second() -> u64 {
    data().map_or(0, |d| d.ticks_per_second)
}

/// Get the data cache line size.
#[no_mangle]
pub extern "C" fn zx_system_get_dcache_line_size() -> u32 {
    data().map_or(0, |d| d.dcache_line_size)
}

/// Get the system version string length and pointer.
/// Returns a pointer to a NUL-terminated string.
#[no_mangle]
pub extern "C" fn zx_system_get_version_string() -> *const u8 {
    data().map_or(core::ptr::null(), |d| d.version_string.as_ptr())
}

/// Get the current monotonic time in nanoseconds.
///
/// In Fuchsia's real vDSO this reads shared memory + hardware tick
/// counter entirely in userspace. For now, falls back to the kernel
/// syscall.
// TODO: implement userspace-only time reading using VdsoConstants
// tick-to-mono conversion ratios + hardware tick counter.
#[no_mangle]
pub unsafe extern "C" fn zx_clock_get_monotonic() -> i64 {
    extern "C" {
        fn zx_clock_get_monotonic_via_kernel(out: *mut i64) -> i32;
    }
    let mut now: i64 = 0;
    unsafe { zx_clock_get_monotonic_via_kernel(&mut now) };
    now
}

/// Read the hardware tick counter.
///
/// In Fuchsia's real vDSO this reads the hardware counter directly.
/// For now, falls back to the kernel syscall.
// TODO: implement direct hardware counter read per architecture.
#[no_mangle]
pub unsafe extern "C" fn zx_ticks_get() -> i64 {
    extern "C" {
        fn zx_ticks_get_via_kernel(out: *mut i64) -> i32;
    }
    let mut ticks: i64 = 0;
    unsafe { zx_ticks_get_via_kernel(&mut ticks) };
    ticks
}

/// Get CPU feature flags.
#[no_mangle]
pub extern "C" fn zx_system_get_features(kind: u32, features: *mut u32) -> i32 {
    // kind 0 = ZX_FEATURE_KIND_CPU
    if kind != 0 || features.is_null() {
        return -10; // ZX_ERR_INVALID_ARGS
    }
    match data() {
        Some(d) => {
            unsafe { *features = d.features_cpu };
            0 // ZX_OK
        }
        None => -2, // ZX_ERR_NOT_SUPPORTED
    }
}

/// Draw random bytes from the kernel CPRNG.
///
/// Loops calling `zx_cprng_draw_once` in chunks of 256 bytes
/// (ZX_CPRNG_DRAW_MAX_LEN). This matches Fuchsia's vDSO behavior.
#[no_mangle]
pub unsafe extern "C" fn zx_cprng_draw(buffer: *mut u8, length: usize) {
    const MAX_CHUNK: usize = 256;
    extern "C" {
        fn zx_cprng_draw_once(buffer: *mut u8, length: usize) -> i32;
    }
    let mut offset = 0;
    while offset < length {
        let chunk = core::cmp::min(MAX_CHUNK, length - offset);
        let status = unsafe { zx_cprng_draw_once(buffer.add(offset), chunk) };
        if status != 0 {
            // Fatal: CPRNG failure is unrecoverable per Fuchsia spec
            loop {
                core::hint::spin_loop();
            }
        }
        offset += chunk;
    }
}

/// Compute an absolute deadline from a relative duration.
///
/// Returns `now + duration` where `now` is the current monotonic time.
/// Uses `zx_clock_get_monotonic_via_kernel` since we don't yet have
/// userspace-only time reading.
#[no_mangle]
pub unsafe extern "C" fn zx_deadline_after(nanoseconds: i64) -> i64 {
    extern "C" {
        fn zx_clock_get_monotonic_via_kernel(out: *mut i64) -> i32;
    }
    let mut now: i64 = 0;
    unsafe { zx_clock_get_monotonic_via_kernel(&mut now) };
    now.saturating_add(nanoseconds)
}

/// Send a message to a channel and wait for a reply.
///
/// Wraps `zx_channel_call_noretry` with retry logic on interrupt.
#[no_mangle]
pub unsafe extern "C" fn zx_channel_call(
    handle: u32,
    options: u32,
    deadline: i64,
    args: *const u8,
    actual_bytes: *mut u32,
    actual_handles: *mut u32,
) -> i32 {
    extern "C" {
        fn zx_channel_call_noretry(
            handle: u32,
            options: u32,
            deadline: i64,
            args: *const u8,
            actual_bytes: *mut u32,
            actual_handles: *mut u32,
        ) -> i32;
        fn zx_channel_call_finish(
            deadline: i64,
            args: *const u8,
            actual_bytes: *mut u32,
            actual_handles: *mut u32,
        ) -> i32;
    }
    let mut status = unsafe {
        zx_channel_call_noretry(
            handle,
            options,
            deadline,
            args,
            actual_bytes,
            actual_handles,
        )
    };
    // ZX_ERR_INTERNAL_INTR_RETRY (-6) means the call was interrupted
    // and should be retried via channel_call_finish.
    while status == -6 {
        status = unsafe { zx_channel_call_finish(deadline, args, actual_bytes, actual_handles) };
    }
    status
}
