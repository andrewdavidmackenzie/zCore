//! petal sync_completion test -- basic wait/signal tests.
//!
//! Port of basic tests from zircon-tests/src/core_tests/sync_completion.rs.

#![no_std]
#![no_main]
#![allow(dead_code)]

extern crate petal;

use zx::sys::*;

fn pass(name: &[u8]) {
    zx::debug_write(b"  sync_completion_test: ");
    zx::debug_write(name);
    zx::debug_write(b" PASS\n");
}

fn fail(name: &[u8], msg: &[u8]) {
    zx::debug_write(b"FAIL: sync_completion_test: ");
    zx::debug_write(name);
    zx::debug_write(b" - ");
    zx::debug_write(msg);
    zx::debug_write(b"\n");
    zx::Process::exit(1);
}

/// Pre-signaled event: wait returns immediately
fn presignal_single_wait() {
    let name = b"presignal_single_wait";
    let mut h: HandleValue = 0;
    let s = unsafe { zx_event_create(0, &mut h) };
    if s != ZX_OK {
        fail(name, b"create");
    }

    // Signal
    let s = unsafe { zx_object_signal(h, 0, 1 << 24) };
    if s != ZX_OK {
        fail(name, b"signal");
    }

    // Wait should return immediately
    let mut pending: u32 = 0;
    let s = unsafe { zx_object_wait_one(h, 1 << 24, 0, &mut pending) };
    if s != ZX_OK {
        fail(name, b"wait should succeed");
    }

    unsafe { zx_handle_close(h) };
    pass(name);
}

/// Timeout with unsignaled event
fn timeout_single_wait() {
    let name = b"timeout_single_wait";
    let mut h: HandleValue = 0;
    let s = unsafe { zx_event_create(0, &mut h) };
    if s != ZX_OK {
        fail(name, b"create");
    }

    let mut pending: u32 = 0;
    let s = unsafe { zx_object_wait_one(h, 1 << 24, 0, &mut pending) };
    if s != ZX_ERR_TIMED_OUT {
        fail(name, b"expected TIMED_OUT");
    }

    unsafe { zx_handle_close(h) };
    pass(name);
}

/// Signal, clear, verify cleared
fn reset_cycle() {
    let name = b"reset_cycle";
    let mut h: HandleValue = 0;
    let s = unsafe { zx_event_create(0, &mut h) };
    if s != ZX_OK {
        fail(name, b"create");
    }

    // Signal
    let s = unsafe { zx_object_signal(h, 0, 1 << 24) };
    if s != ZX_OK {
        fail(name, b"signal");
    }

    // Should be signaled
    let mut pending: u32 = 0;
    let s = unsafe { zx_object_wait_one(h, 1 << 24, 0, &mut pending) };
    if s != ZX_OK {
        fail(name, b"wait after signal");
    }

    // Clear
    let s = unsafe { zx_object_signal(h, 1 << 24, 0) };
    if s != ZX_OK {
        fail(name, b"clear");
    }

    // Should time out now
    let s = unsafe { zx_object_wait_one(h, 1 << 24, 0, &mut pending) };
    if s != ZX_ERR_TIMED_OUT {
        fail(name, b"should time out after clear");
    }

    unsafe { zx_handle_close(h) };
    pass(name);
}

#[no_mangle]
pub fn main() {
    zx::debug_write(b"sync_completion_test: starting\n");
    presignal_single_wait();
    timeout_single_wait();
    reset_cycle();
    zx::debug_write(b"sync_completion_test: all tests passed\n");
}
