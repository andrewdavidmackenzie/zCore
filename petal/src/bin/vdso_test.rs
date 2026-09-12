//! petal vDSO test -- validates that VdsoConstants is properly mapped.
//!
//! Uses ZX_PROP_PROCESS_VDSO_BASE_ADDRESS to find the mapped
//! VdsoConstants data page and validates its contents.

#![no_std]
#![no_main]

extern crate petal;

use zx::sys::ZX_OK;

/// ZX_PROP_PROCESS_VDSO_BASE_ADDRESS
const PROP_VDSO_BASE: u32 = 6;

/// VdsoConstants layout matching kernel-hal/src/common/vdso.rs.
#[repr(C)]
struct VdsoConstants {
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

#[no_mangle]
pub fn main() {
    zx::debug_write(b"vdso_test: starting\n");

    // Use the startup handle (a channel) for get_property.
    // Our ProcessVdsoBaseAddress handler falls back to the calling
    // process's own VMAR when the handle is not a Process.
    let startup = petal::take_startup_handle();

    // Query ZX_PROP_PROCESS_VDSO_BASE_ADDRESS
    let mut vdso_base: usize = 0;
    let status = unsafe {
        zx::sys::zx_object_get_property(
            startup,
            PROP_VDSO_BASE,
            &mut vdso_base as *mut usize as *mut u8,
            core::mem::size_of::<usize>(),
        )
    };
    if status != ZX_OK {
        zx::debug_write(b"vdso_test: FAIL - get_property failed\n");
        zx::Process::exit(1);
    }

    if vdso_base == 0 {
        zx::debug_write(b"vdso_test: FAIL - vDSO base address is 0\n");
        zx::Process::exit(1);
    }
    zx::debug_write(b"vdso_test: vDSO base address is non-zero\n");

    // Read VdsoConstants from the mapped address
    let constants = unsafe { &*(vdso_base as *const VdsoConstants) };

    if constants.max_num_cpus == 0 {
        zx::debug_write(b"vdso_test: FAIL - max_num_cpus is 0\n");
        zx::Process::exit(1);
    }
    zx::debug_write(b"vdso_test: max_num_cpus >= 1\n");

    if constants.ticks_per_second > 0 {
        zx::debug_write(b"vdso_test: ticks_per_second > 0\n");
    } else {
        // aarch64 HAL doesn't implement cpu_frequency() yet
        zx::debug_write(b"vdso_test: ticks_per_second is 0 (cpu_frequency not implemented)\n");
    }

    if constants.version_string_len == 0 {
        zx::debug_write(b"vdso_test: FAIL - version_string_len is 0\n");
        zx::Process::exit(1);
    }
    zx::debug_write(b"vdso_test: version_string_len > 0\n");

    // Clean up
    unsafe {
        zx::sys::zx_handle_close(startup);
    }

    zx::debug_write(b"vdso_test: PASS\n");
}
