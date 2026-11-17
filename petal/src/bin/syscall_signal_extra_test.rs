//! petal signal extra test -- event/timer create and signal operations.
//!
//! Port of zircon-tests/src/syscall_signal_extra.rs.

#![no_std]
#![no_main]
#![allow(dead_code)]

extern crate petal;

use zx::sys::*;

fn pass(name: &[u8]) {
    zx::debug_write(b"  signal_extra_test: ");
    zx::debug_write(name);
    zx::debug_write(b" PASS\n");
}

fn fail(name: &[u8], msg: &[u8]) {
    zx::debug_write(b"FAIL: signal_extra_test: ");
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

/// Event create via syscall
fn event_create_via_syscall() {
    let name = b"event_create_via_syscall";
    let mut h: HandleValue = 0;
    assert_ok(unsafe { zx_event_create(0, &mut h) }, name, b"");
    if h == 0 {
        fail(name, b"handle is 0");
    }
    unsafe { zx_handle_close(h) };
    pass(name);
}

/// Event signal self
fn event_signal_self() {
    let name = b"event_signal_self";
    let mut h: HandleValue = 0;
    assert_ok(unsafe { zx_event_create(0, &mut h) }, name, b"create");
    // Signal USER_SIGNAL_0
    assert_ok(unsafe { zx_object_signal(h, 0, 1 << 24) }, name, b"signal");
    // Clear
    assert_ok(unsafe { zx_object_signal(h, 1 << 24, 0) }, name, b"clear");
    unsafe { zx_handle_close(h) };
    pass(name);
}

/// Timer create via syscall
fn timer_create_via_syscall() {
    let name = b"timer_create_via_syscall";
    let mut h: HandleValue = 0;
    assert_ok(unsafe { zx_timer_create(0, 0, &mut h) }, name, b"");
    if h == 0 {
        fail(name, b"handle is 0");
    }
    unsafe { zx_handle_close(h) };
    pass(name);
}

/// Timer cancel via syscall
fn timer_cancel_via_syscall() {
    let name = b"timer_cancel_via_syscall";
    let mut h: HandleValue = 0;
    assert_ok(unsafe { zx_timer_create(0, 0, &mut h) }, name, b"create");
    assert_ok(unsafe { zx_timer_cancel(h) }, name, b"cancel");
    unsafe { zx_handle_close(h) };
    pass(name);
}

/// Eventpair invalid signal bits
fn eventpair_invalid_signal_bits() {
    let name = b"eventpair_invalid_signal_bits";
    let mut h0: HandleValue = 0;
    let mut h1: HandleValue = 0;
    assert_ok(
        unsafe { zx_eventpair_create(0, &mut h0, &mut h1) },
        name,
        b"create",
    );

    // PEER_CLOSED (bit 2) cannot be set by userspace
    assert_status(
        unsafe { zx_object_signal(h0, 0, 1 << 2) },
        ZX_ERR_INVALID_ARGS,
        name,
        b"set PEER_CLOSED",
    );

    // Non-user bit 4 -> INVALID_ARGS
    assert_status(
        unsafe { zx_object_signal(h0, 0, 1 << 4) },
        ZX_ERR_INVALID_ARGS,
        name,
        b"non-user bit 4",
    );
    assert_status(
        unsafe { zx_object_signal_peer(h0, 0, 1 << 4) },
        ZX_ERR_INVALID_ARGS,
        name,
        b"peer non-user bit 4",
    );

    // SIGNALED (bit 3) IS allowed
    assert_ok(
        unsafe { zx_object_signal(h0, 0, 1 << 3) },
        name,
        b"set SIGNALED",
    );
    assert_ok(
        unsafe { zx_object_signal(h0, 1 << 3, 0) },
        name,
        b"clear SIGNALED",
    );

    unsafe { zx_handle_close(h0) };
    unsafe { zx_handle_close(h1) };
    pass(name);
}

#[no_mangle]
pub fn main() {
    zx::debug_write(b"signal_extra_test: starting\n");

    event_create_via_syscall();
    event_signal_self();
    timer_create_via_syscall();
    timer_cancel_via_syscall();
    eventpair_invalid_signal_bits();

    zx::debug_write(b"signal_extra_test: all tests passed\n");
}
