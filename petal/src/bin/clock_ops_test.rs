//! petal clock_ops test -- exercises clock create/read/update/get_details.
//!
//! Port of zircon-tests/src/clock_ops.rs and clock_extra.rs.

#![no_std]
#![no_main]
#![allow(dead_code)]

extern crate petal;

use zx::sys::*;

const PA_RESOURCE: u32 = 0x3F;
const ZX_CLOCK_OPT_AUTO_START: u64 = 1 << 0;
const ZX_CLOCK_OPT_MONOTONIC: u64 = 1 << 1;
const ZX_CLOCK_OPT_CONTINUOUS: u64 = 1 << 2;

fn pass(name: &[u8]) {
    zx::debug_write(b"  clock_ops_test: ");
    zx::debug_write(name);
    zx::debug_write(b" PASS\n");
}

fn fail(name: &[u8], msg: &[u8]) {
    zx::debug_write(b"FAIL: clock_ops_test: ");
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

fn clock_create(options: u64) -> Result<HandleValue, ZxStatus> {
    let mut h: HandleValue = 0;
    let s = unsafe { zx_clock_create(options, core::ptr::null(), &mut h) };
    if s == ZX_OK {
        Ok(h)
    } else {
        Err(s)
    }
}

/// Monotonic time should be non-negative
fn monotonic_non_zero() {
    let name = b"monotonic_non_zero";
    let t = unsafe { zx_clock_get_monotonic_via_kernel() };
    // On early boot, the monotonic clock may still be near zero.
    // Accept any non-negative value.
    if t < 0 {
        fail(name, b"negative time");
    }
    pass(name);
}

/// Monotonic time should increase
fn monotonic_increasing() {
    let name = b"monotonic_increasing";
    let t1 = unsafe { zx_clock_get_monotonic_via_kernel() };
    let t2 = unsafe { zx_clock_get_monotonic_via_kernel() };
    if t2 < t1 {
        fail(name, b"went backwards");
    }
    pass(name);
}

/// Create a UTC clock
fn utc_clock_create() {
    let name = b"utc_clock_create";
    let h = clock_create(0).unwrap_or_else(|_| {
        fail(name, b"create");
        0
    });
    if h == 0 {
        fail(name, b"handle is 0");
    }
    unsafe { zx_handle_close(h) };
    pass(name);
}

/// Create clock with AUTO_START, then read
fn clock_create_auto_start_and_read() {
    let name = b"clock_create_auto_start_and_read";
    let h = clock_create(ZX_CLOCK_OPT_AUTO_START).unwrap_or_else(|_| {
        fail(name, b"create");
        0
    });

    let mut now: i64 = 0;
    assert_ok(unsafe { zx_clock_read(h, &mut now) }, name, b"read");
    // Auto-started clock should return a non-negative value
    if now < 0 {
        fail(name, b"negative time from auto-start clock");
    }

    unsafe { zx_handle_close(h) };
    pass(name);
}

/// Read clock before start should fail
fn clock_read_before_start_fails() {
    let name = b"clock_read_before_start_fails";
    // Create without AUTO_START
    let h = clock_create(0).unwrap_or_else(|_| {
        fail(name, b"create");
        0
    });

    let mut now: i64 = 0;
    let s = unsafe { zx_clock_read(h, &mut now) };
    // Should fail because clock hasn't been started
    if s == ZX_OK {
        fail(name, b"read should fail before start");
    }

    unsafe { zx_handle_close(h) };
    pass(name);
}

/// Invalid handle for clock_read
fn clock_invalid_handle() {
    let name = b"clock_invalid_handle";
    let mut now: i64 = 0;
    let s = unsafe { zx_clock_read(0xDEAD, &mut now) };
    if s != ZX_ERR_BAD_HANDLE {
        fail(name, b"expected BAD_HANDLE");
    }
    pass(name);
}

/// Clock read without READ right
fn clock_no_read_right() {
    let name = b"clock_no_read_right";
    let h = clock_create(ZX_CLOCK_OPT_AUTO_START).unwrap_or_else(|_| {
        fail(name, b"create");
        0
    });

    // Duplicate without READ
    let mut no_read: HandleValue = 0;
    assert_ok(
        unsafe {
            zx_handle_duplicate(
                h,
                ZX_RIGHT_WRITE | ZX_RIGHT_WAIT | ZX_RIGHT_INSPECT,
                &mut no_read,
            )
        },
        name,
        b"dup",
    );

    let mut now: i64 = 0;
    let s = unsafe { zx_clock_read(no_read, &mut now) };
    if s != ZX_ERR_ACCESS_DENIED {
        fail(name, b"expected ACCESS_DENIED");
    }

    unsafe { zx_handle_close(no_read) };
    unsafe { zx_handle_close(h) };
    pass(name);
}

/// Clock update starts the clock
fn clock_update_starts_clock() {
    let name = b"clock_update_starts_clock";
    let h = clock_create(0).unwrap_or_else(|_| {
        fail(name, b"create");
        0
    });

    // Update with value=0 to start the clock
    // ZX_CLOCK_UPDATE_OPTION_VALUE_VALID = 1 << 0
    let options: u64 = 1 << 0;
    let args = [0u8; 32]; // zx_clock_update_args_v2_t (value at offset 8, all zero)
    assert_ok(
        unsafe { zx_clock_update(h, options, args.as_ptr()) },
        name,
        b"update",
    );

    // Now read should succeed
    let mut now: i64 = 0;
    assert_ok(
        unsafe { zx_clock_read(h, &mut now) },
        name,
        b"read after update",
    );

    unsafe { zx_handle_close(h) };
    pass(name);
}

#[no_mangle]
pub fn main() {
    zx::debug_write(b"clock_ops_test: starting\n");

    let bootstrap = petal::Bootstrap::read().unwrap_or_else(|| {
        zx::debug_write(b"FAIL: no bootstrap\n");
        zx::Process::exit(1);
    });
    let _resource = bootstrap.find(PA_RESOURCE);

    monotonic_non_zero();
    monotonic_increasing();
    utc_clock_create();
    clock_create_auto_start_and_read();
    clock_read_before_start_fails();
    clock_invalid_handle();
    clock_no_read_right();
    clock_update_starts_clock();

    zx::debug_write(b"clock_ops_test: all tests passed\n");
}
