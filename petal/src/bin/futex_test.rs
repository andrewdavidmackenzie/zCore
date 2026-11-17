//! petal futex test -- futex wait/wake/requeue from user mode.
//!
//! Port of sync tests from zircon-tests/src/core_tests/futex.rs.

#![no_std]
#![no_main]
#![allow(dead_code)]

extern crate petal;

use zx::sys::*;

fn pass(name: &[u8]) {
    zx::debug_write(b"  futex_test: ");
    zx::debug_write(name);
    zx::debug_write(b" PASS\n");
}

fn fail(name: &[u8], msg: &[u8]) {
    zx::debug_write(b"FAIL: futex_test: ");
    zx::debug_write(name);
    zx::debug_write(b" - ");
    zx::debug_write(msg);
    zx::debug_write(b"\n");
    zx::Process::exit(1);
}

/// Wait with value mismatch should return BAD_STATE immediately
fn wait_value_mismatch() {
    let name = b"wait_value_mismatch";
    let futex_val: i32 = 42;
    // Expected value is 99 but actual is 42 -> BAD_STATE
    let s = unsafe {
        zx_futex_wait(&futex_val as *const i32, 99, 0, 0 /* deadline=0 */)
    };
    if s != ZX_ERR_BAD_STATE {
        fail(name, b"expected BAD_STATE");
    }
    pass(name);
}

/// Wait with bad (null) address
fn wait_bad_address() {
    let name = b"wait_bad_address";
    let s = unsafe { zx_futex_wait(core::ptr::null(), 0, 0, 0) };
    // Should fail — null address is invalid
    if s == ZX_OK {
        fail(name, b"null address should fail");
    }
    pass(name);
}

/// Misaligned futex address
fn misaligned_futex_addr() {
    let name = b"misaligned_futex_addr";
    // Create a buffer and use a misaligned pointer
    let buf = [0u8; 8];
    let misaligned = unsafe { buf.as_ptr().add(1) as *const i32 };
    let s = unsafe { zx_futex_wait(misaligned, 0, 0, 0) };
    // Should fail with INVALID_ARGS (misaligned)
    if s == ZX_OK {
        fail(name, b"misaligned should fail");
    }
    pass(name);
}

/// Wake with count=0 wakes nobody (no-op)
fn wake_with_wake_count_zero() {
    let name = b"wake_with_wake_count_zero";
    let futex_val: i32 = 0;
    let s = unsafe { zx_futex_wake(&futex_val as *const i32, 0) };
    if s != ZX_OK {
        fail(name, b"wake 0 should succeed");
    }
    pass(name);
}

/// Wake with valid address (nobody waiting, should succeed)
fn wakeup_address() {
    let name = b"wakeup_address";
    let futex_val: i32 = 0;
    let s = unsafe { zx_futex_wake(&futex_val as *const i32, 1) };
    if s != ZX_OK {
        fail(name, b"wake should succeed");
    }
    pass(name);
}

/// Wait with immediate timeout
fn wait_timeout() {
    let name = b"wait_timeout";
    let futex_val: i32 = 0;
    // Expected value matches, deadline=0 -> should time out immediately
    let s = unsafe { zx_futex_wait(&futex_val as *const i32, 0, 0, 0) };
    if s != ZX_ERR_TIMED_OUT {
        fail(name, b"expected TIMED_OUT");
    }
    pass(name);
}

/// Requeue with value mismatch -> BAD_STATE
fn requeue_value_mismatch() {
    let name = b"requeue_value_mismatch";
    let wake_addr: i32 = 42;
    let requeue_addr: i32 = 0;
    // Value mismatch: actual=42, expected=99
    let s = unsafe {
        zx_futex_requeue(
            &wake_addr as *const i32,
            0,  // wake_count
            99, // current_value (mismatches 42)
            &requeue_addr as *const i32,
            0, // requeue_count
            0, // new_requeue_owner
        )
    };
    if s != ZX_ERR_BAD_STATE {
        fail(name, b"expected BAD_STATE");
    }
    pass(name);
}

/// Requeue same address -> INVALID_ARGS
fn requeue_same_addr() {
    let name = b"requeue_same_addr";
    let futex_val: i32 = 0;
    let s = unsafe {
        zx_futex_requeue(
            &futex_val as *const i32,
            0,
            0,
            &futex_val as *const i32, // same address
            0,
            0,
        )
    };
    if s != ZX_ERR_INVALID_ARGS {
        fail(name, b"expected INVALID_ARGS");
    }
    pass(name);
}

#[no_mangle]
pub fn main() {
    zx::debug_write(b"futex_test: starting\n");

    wait_value_mismatch();
    wait_bad_address();
    misaligned_futex_addr();
    wake_with_wake_count_zero();
    wakeup_address();
    wait_timeout();
    requeue_value_mismatch();
    requeue_same_addr();

    zx::debug_write(b"futex_test: all tests passed\n");
}
