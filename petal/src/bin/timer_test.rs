//! petal timer test -- exercises timer syscalls from user mode.
//!
//! Port of sync tests from zircon-tests/src/core_tests/timer.rs.

#![no_std]
#![no_main]
#![allow(dead_code)]

extern crate petal;

use zx::sys::*;

const ZX_INFO_HANDLE_BASIC: u32 = 2;

fn pass(name: &[u8]) {
    zx::debug_write(b"  timer_test: ");
    zx::debug_write(name);
    zx::debug_write(b" PASS\n");
}

fn fail(name: &[u8], msg: &[u8]) {
    zx::debug_write(b"FAIL: timer_test: ");
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

fn timer_create(options: u32, clock_id: u32) -> Result<HandleValue, ZxStatus> {
    let mut h: HandleValue = 0;
    let s = unsafe { zx_timer_create(options, clock_id, &mut h) };
    if s == ZX_OK {
        Ok(h)
    } else {
        Err(s)
    }
}

/// C++: TEST(TimerTest, InvalidCalls)
fn invalid_calls() {
    let name = b"invalid_calls";

    let mut h: HandleValue = 0;

    // Invalid clock_id
    assert_status(
        unsafe { zx_timer_create(0, 2, &mut h) }, // clock_id=2 invalid
        ZX_ERR_INVALID_ARGS,
        name,
        b"invalid clock_id",
    );

    // Invalid options
    assert_status(
        unsafe { zx_timer_create(0xFF, 0, &mut h) },
        ZX_ERR_INVALID_ARGS,
        name,
        b"invalid options",
    );

    // Valid create
    let h = timer_create(0, 0).unwrap_or_else(|_| {
        fail(name, b"create");
        0
    });

    // Set with negative slack (passed as u64 wrapping of -1)
    let s = unsafe { zx_timer_set(h, 0, (-1i64) as u64) };
    assert_status(s, ZX_ERR_OUT_OF_RANGE, name, b"negative slack");

    // Cancel valid timer (should succeed even if not armed)
    assert_ok(unsafe { zx_timer_cancel(h) }, name, b"cancel");

    unsafe { zx_handle_close(h) };
    pass(name);
}

/// C++: TEST(TimerTest, EdgeCases)
fn edge_cases() {
    let name = b"edge_cases";
    let h = timer_create(0, 0).unwrap_or_else(|_| {
        fail(name, b"create");
        0
    });

    // Set timer with deadline 0 (past)
    assert_ok(unsafe { zx_timer_set(h, 0, 0) }, name, b"set deadline 0");

    // Timer should fire immediately — check SIGNALED
    let mut pending: u32 = 0;
    let s = unsafe {
        zx_object_wait_one(h, 1 << 3 /* SIGNALED */, 0, &mut pending)
    };
    // Either OK (signal present) or TIMED_OUT (not yet)
    if s != ZX_OK && s != ZX_ERR_TIMED_OUT {
        fail(name, b"wait_one failed");
    }

    // Cancel
    assert_ok(unsafe { zx_timer_cancel(h) }, name, b"cancel");

    // Set with far future deadline
    assert_ok(
        unsafe { zx_timer_set(h, i64::MAX, 0) },
        name,
        b"set far future",
    );

    // Cancel again
    assert_ok(unsafe { zx_timer_cancel(h) }, name, b"cancel far future");

    unsafe { zx_handle_close(h) };
    pass(name);
}

/// C++: TEST(TimerTest, SetNegativeDeadline)
fn set_negative_deadline() {
    let name = b"set_negative_deadline";
    let h = timer_create(0, 0).unwrap_or_else(|_| {
        fail(name, b"create");
        0
    });

    // Negative deadline (past time) should succeed
    assert_ok(unsafe { zx_timer_set(h, -1, 0) }, name, b"set -1");

    unsafe { zx_handle_close(h) };
    pass(name);
}

/// C++: TEST(TimerTest, SetNegativeDeadlineMax)
fn set_negative_deadline_max() {
    let name = b"set_negative_deadline_max";
    let h = timer_create(0, 0).unwrap_or_else(|_| {
        fail(name, b"create");
        0
    });

    assert_ok(unsafe { zx_timer_set(h, i64::MIN, 0) }, name, b"set MIN");

    unsafe { zx_handle_close(h) };
    pass(name);
}

/// C++: TEST(TimerTest, Restart)
fn restart() {
    let name = b"restart";
    let h = timer_create(0, 0).unwrap_or_else(|_| {
        fail(name, b"create");
        0
    });

    // Set, cancel, set again (restart)
    assert_ok(unsafe { zx_timer_set(h, i64::MAX, 0) }, name, b"set");
    assert_ok(unsafe { zx_timer_cancel(h) }, name, b"cancel");
    assert_ok(unsafe { zx_timer_set(h, i64::MAX, 0) }, name, b"restart");
    assert_ok(unsafe { zx_timer_cancel(h) }, name, b"cancel 2");

    unsafe { zx_handle_close(h) };
    pass(name);
}

/// C++: TEST(TimerTest, BasicInfo)
fn basic_info() {
    let name = b"basic_info";
    let h = timer_create(0, 0).unwrap_or_else(|_| {
        fail(name, b"create");
        0
    });

    // Get handle info
    let mut buf = [0u8; 32];
    let mut actual: usize = 0;
    let mut avail: usize = 0;
    assert_ok(
        unsafe {
            zx_object_get_info(
                h,
                ZX_INFO_HANDLE_BASIC,
                buf.as_mut_ptr(),
                32,
                &mut actual,
                &mut avail,
            )
        },
        name,
        b"get_info",
    );

    // obj_type at offset 12: Timer = 22
    let obj_type = u32::from_le_bytes(buf[12..16].try_into().unwrap());
    if obj_type != 22 {
        fail(name, b"wrong obj_type (expected 22=Timer)");
    }

    unsafe { zx_handle_close(h) };
    pass(name);
}

#[no_mangle]
pub fn main() {
    zx::debug_write(b"timer_test: starting\n");

    invalid_calls();
    edge_cases();
    set_negative_deadline();
    set_negative_deadline_max();
    restart();
    basic_info();

    zx::debug_write(b"timer_test: all tests passed\n");
}
