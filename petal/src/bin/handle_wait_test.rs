//! petal handle_wait test -- wait_one/wait_many basic validation.
//!
//! Sync portions of zircon-tests/src/core_tests/handle_wait.rs.

#![no_std]
#![no_main]
#![allow(dead_code)]

extern crate petal;

use zx::sys::*;

fn pass(name: &[u8]) {
    zx::debug_write(b"  handle_wait_test: ");
    zx::debug_write(name);
    zx::debug_write(b" PASS\n");
}

fn fail(name: &[u8], msg: &[u8]) {
    zx::debug_write(b"FAIL: handle_wait_test: ");
    zx::debug_write(name);
    zx::debug_write(b" - ");
    zx::debug_write(msg);
    zx::debug_write(b"\n");
    zx::Process::exit(1);
}

/// Wait on bad handle -> BAD_HANDLE
fn wait_bad_handle() {
    let name = b"wait_bad_handle";
    let mut pending: u32 = 0;
    let s = unsafe { zx_object_wait_one(0xDEAD, 0xFFFF, 0, &mut pending) };
    if s != ZX_ERR_BAD_HANDLE {
        fail(name, b"expected BAD_HANDLE");
    }
    pass(name);
}

/// Wait without WAIT right -> ACCESS_DENIED
fn wait_no_wait_right() {
    let name = b"wait_no_wait_right";
    let mut h: HandleValue = 0;
    let s = unsafe { zx_event_create(0, &mut h) };
    if s != ZX_OK {
        fail(name, b"event create");
    }

    // Dup without WAIT right
    let mut no_wait: HandleValue = 0;
    let s = unsafe { zx_handle_duplicate(h, ZX_RIGHT_SIGNAL | ZX_RIGHT_INSPECT, &mut no_wait) };
    if s != ZX_OK {
        fail(name, b"dup");
    }

    let mut pending: u32 = 0;
    let s = unsafe { zx_object_wait_one(no_wait, 1 << 24, 0, &mut pending) };
    if s != ZX_ERR_ACCESS_DENIED {
        fail(name, b"expected ACCESS_DENIED");
    }

    unsafe { zx_handle_close(no_wait) };
    unsafe { zx_handle_close(h) };
    pass(name);
}

/// Wait with timeout=0 on unsignaled event -> TIMED_OUT
fn wait_timeout() {
    let name = b"wait_timeout";
    let mut h: HandleValue = 0;
    let s = unsafe { zx_event_create(0, &mut h) };
    if s != ZX_OK {
        fail(name, b"event create");
    }

    let mut pending: u32 = 0;
    let s = unsafe { zx_object_wait_one(h, 1 << 24, 0, &mut pending) };
    if s != ZX_ERR_TIMED_OUT {
        fail(name, b"expected TIMED_OUT");
    }

    unsafe { zx_handle_close(h) };
    pass(name);
}

/// Signal event then wait -> should succeed immediately
fn wait_already_signaled() {
    let name = b"wait_already_signaled";
    let mut h: HandleValue = 0;
    let s = unsafe { zx_event_create(0, &mut h) };
    if s != ZX_OK {
        fail(name, b"event create");
    }

    // Signal USER_SIGNAL_0
    let s = unsafe { zx_object_signal(h, 0, 1 << 24) };
    if s != ZX_OK {
        fail(name, b"signal");
    }

    // Wait should succeed immediately
    let mut pending: u32 = 0;
    let s = unsafe { zx_object_wait_one(h, 1 << 24, 0, &mut pending) };
    if s != ZX_OK {
        fail(name, b"wait should succeed");
    }
    if pending & (1 << 24) == 0 {
        fail(name, b"signal not in pending");
    }

    unsafe { zx_handle_close(h) };
    pass(name);
}

#[no_mangle]
pub fn main() {
    zx::debug_write(b"handle_wait_test: starting\n");

    wait_bad_handle();
    wait_no_wait_right();
    wait_timeout();
    wait_already_signaled();

    zx::debug_write(b"handle_wait_test: all tests passed\n");
}
