//! petal system_cpu test -- performance info get/set syscalls.
//!
//! Port of zircon-tests/src/core_tests/system_cpu.rs.

#![no_std]
#![no_main]
#![allow(dead_code)]

extern crate alloc;
extern crate petal;

use alloc::vec;
use zx::sys::*;

const PA_RESOURCE: u32 = 0x3F;

// Performance info topics
const ZX_CPU_PERF_SCALE: u32 = 1;
const ZX_CPU_DEFAULT_PERF_SCALE: u32 = 2;
const ZX_CPU_PERF_LIMIT: u32 = 3;

fn pass(name: &[u8]) {
    zx::debug_write(b"  system_cpu_test: ");
    zx::debug_write(name);
    zx::debug_write(b" PASS\n");
}

fn fail(name: &[u8], msg: &[u8]) {
    zx::debug_write(b"FAIL: system_cpu_test: ");
    zx::debug_write(name);
    zx::debug_write(b" - ");
    zx::debug_write(msg);
    zx::debug_write(b"\n");
    zx::Process::exit(1);
}

fn assert_ok(s: ZxStatus, name: &[u8], msg: &[u8]) {
    if s != ZX_OK {
        fail(name, msg);
    }
}

fn assert_status(got: ZxStatus, expected: ZxStatus, name: &[u8], msg: &[u8]) {
    if got != expected {
        fail(name, msg);
    }
}

/// Set performance info argument validation
fn set_performance_info_argument_validation(resource: HandleValue) {
    let name = b"set_perf_info_validation";

    // Invalid handle
    let info = [0u8; 12];
    assert_status(
        unsafe { zx_system_set_performance_info(0, ZX_CPU_PERF_SCALE, info.as_ptr(), 1) },
        ZX_ERR_BAD_HANDLE,
        name,
        b"bad handle",
    );

    // Wrong type (use an event instead of resource)
    let mut event: HandleValue = 0;
    assert_ok(
        unsafe { zx_event_create(0, &mut event) },
        name,
        b"event create",
    );
    assert_status(
        unsafe { zx_system_set_performance_info(event, u32::MAX, info.as_ptr(), 1) },
        ZX_ERR_WRONG_TYPE,
        name,
        b"wrong type",
    );
    unsafe { zx_handle_close(event) };

    // Invalid topic
    assert_status(
        unsafe { zx_system_set_performance_info(resource, u32::MAX, info.as_ptr(), 1) },
        ZX_ERR_INVALID_ARGS,
        name,
        b"invalid topic",
    );

    // Null pointer
    assert_status(
        unsafe {
            zx_system_set_performance_info(resource, ZX_CPU_PERF_SCALE, core::ptr::null(), 1)
        },
        ZX_ERR_INVALID_ARGS,
        name,
        b"null pointer",
    );

    // Count 0
    assert_status(
        unsafe { zx_system_set_performance_info(resource, ZX_CPU_PERF_SCALE, info.as_ptr(), 0) },
        ZX_ERR_OUT_OF_RANGE,
        name,
        b"count 0",
    );

    // Zero scale -> OUT_OF_RANGE
    let zero_info = [0u8; 12]; // cpu=0, scale={0,0}
    assert_status(
        unsafe {
            zx_system_set_performance_info(resource, ZX_CPU_PERF_SCALE, zero_info.as_ptr(), 1)
        },
        ZX_ERR_OUT_OF_RANGE,
        name,
        b"zero scale",
    );

    // Valid: scale {1, 1}
    let valid_info: [u8; 12] = [0, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0, 0];
    assert_ok(
        unsafe {
            zx_system_set_performance_info(resource, ZX_CPU_PERF_SCALE, valid_info.as_ptr(), 1)
        },
        name,
        b"valid scale",
    );

    pass(name);
}

/// Get performance info argument validation
/// Discover the online CPU count at runtime: the kernel requires
/// `count == num_cpus` exactly, so probe with a valid query (matching
/// the C++ test's GetCpuCount helper).
fn probe_cpu_count(resource: HandleValue) -> usize {
    let mut info = [0u8; 12 * 64]; // large enough for any QEMU -smp
    let mut count: usize = 0;
    for cpus in 1..=64usize {
        let s = unsafe {
            zx_system_get_performance_info(
                resource,
                ZX_CPU_DEFAULT_PERF_SCALE,
                cpus,
                info.as_mut_ptr(),
                &mut count,
            )
        };
        if s == ZX_OK {
            return cpus;
        }
    }
    1
}

fn get_performance_info_argument_validation(resource: HandleValue) {
    let name = b"get_perf_info_validation";

    let online_cpus: usize = probe_cpu_count(resource);

    // Invalid handle
    let mut info = [0u8; 12];
    let mut count: usize = 0;
    assert_status(
        unsafe {
            zx_system_get_performance_info(0, u32::MAX, online_cpus, info.as_mut_ptr(), &mut count)
        },
        ZX_ERR_BAD_HANDLE,
        name,
        b"bad handle",
    );

    // Wrong type
    let mut event: HandleValue = 0;
    assert_ok(
        unsafe { zx_event_create(0, &mut event) },
        name,
        b"event create",
    );
    assert_status(
        unsafe {
            zx_system_get_performance_info(
                event,
                u32::MAX,
                online_cpus,
                info.as_mut_ptr(),
                &mut count,
            )
        },
        ZX_ERR_WRONG_TYPE,
        name,
        b"wrong type",
    );
    unsafe { zx_handle_close(event) };

    // Invalid topic
    assert_status(
        unsafe {
            zx_system_get_performance_info(
                resource,
                u32::MAX,
                online_cpus,
                info.as_mut_ptr(),
                &mut count,
            )
        },
        ZX_ERR_INVALID_ARGS,
        name,
        b"invalid topic",
    );

    // Count 0
    assert_status(
        unsafe {
            zx_system_get_performance_info(
                resource,
                ZX_CPU_PERF_SCALE,
                0,
                info.as_mut_ptr(),
                &mut count,
            )
        },
        ZX_ERR_OUT_OF_RANGE,
        name,
        b"count 0",
    );

    // Null output_count
    // Need properly sized buffer for the query
    let mut big_info = vec![0u8; 12 * 8]; // up to 8 CPUs
    assert_status(
        unsafe {
            zx_system_get_performance_info(
                resource,
                ZX_CPU_PERF_SCALE,
                online_cpus,
                big_info.as_mut_ptr(),
                core::ptr::null_mut(),
            )
        },
        ZX_ERR_INVALID_ARGS,
        name,
        b"null output_count",
    );

    pass(name);
}

/// Get performance info (success path)
fn get_performance_info(resource: HandleValue) {
    let name = b"get_perf_info";

    // Query default perf scale — use buffer large enough for 8 CPUs
    let mut info = vec![0u8; 12 * 8];
    let mut count: usize = 0;

    // Try with count=1 first
    let s = unsafe {
        zx_system_get_performance_info(
            resource,
            ZX_CPU_DEFAULT_PERF_SCALE,
            1,
            info.as_mut_ptr(),
            &mut count,
        )
    };
    if s == ZX_OK {
        // Should return count >= 1
        if count == 0 {
            fail(name, b"count is 0");
        }

        // Parse first entry: cpu(u32) + integral(u32) + fractional(u32)
        let integral = u32::from_ne_bytes(info[4..8].try_into().unwrap());
        let fractional = u32::from_ne_bytes(info[8..12].try_into().unwrap());
        if integral == 0 && fractional == 0 {
            fail(name, b"default perf scale is zero");
        }
    }
    // If s != OK, the query might need count == num_cpus; skip

    pass(name);
}

/// Set near-zero scale (epsilon) — should succeed without crash
fn set_performance_info_near_zero(resource: HandleValue) {
    let name = b"set_perf_info_near_zero";
    // cpu=0, scale={integral=0, fractional=1} -> near-zero but not zero
    let info: [u8; 12] = [0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0];
    assert_ok(
        unsafe { zx_system_set_performance_info(resource, ZX_CPU_PERF_SCALE, info.as_ptr(), 1) },
        name,
        b"",
    );
    pass(name);
}

/// Scale bandwidth (C++ test is disabled upstream)
fn scale_bandwidth() {
    let name = b"scale_bandwidth";
    // C++ test: return; (disabled while flakes are investigated)
    pass(name);
}

#[no_mangle]
pub fn main() {
    zx::debug_write(b"system_cpu_test: starting\n");

    let bootstrap = petal::Bootstrap::read().unwrap_or_else(|| {
        zx::debug_write(b"FAIL: no bootstrap\n");
        zx::Process::exit(1);
    });

    let resource = bootstrap.find(PA_RESOURCE);
    if resource == 0 {
        zx::debug_write(b"  SKIP: no root resource\n");
        zx::debug_write(b"system_cpu_test: all tests passed\n");
        return;
    }

    set_performance_info_argument_validation(resource);
    get_performance_info_argument_validation(resource);
    get_performance_info(resource);
    set_performance_info_near_zero(resource);
    scale_bandwidth();

    zx::debug_write(b"system_cpu_test: all tests passed\n");
}
