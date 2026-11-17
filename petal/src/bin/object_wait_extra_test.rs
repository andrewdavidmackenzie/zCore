//! petal object_wait_extra test -- wait_async/cancel tests.
//!
//! Port of sync portions from zircon-tests/src/object_wait_extra.rs.

#![no_std]
#![no_main]
#![allow(dead_code)]

extern crate petal;

use zx::sys::*;

fn pass(name: &[u8]) {
    zx::debug_write(b"  obj_wait_extra_test: ");
    zx::debug_write(name);
    zx::debug_write(b" PASS\n");
}

fn fail(name: &[u8], msg: &[u8]) {
    zx::debug_write(b"FAIL: obj_wait_extra_test: ");
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

/// Wait async and cancel
fn wait_async_cancel() {
    let name = b"wait_async_cancel";
    let mut port: HandleValue = 0;
    let mut event: HandleValue = 0;
    assert_ok(
        unsafe { zx_port_create(0, &mut port) },
        name,
        b"port create",
    );
    assert_ok(
        unsafe { zx_event_create(0, &mut event) },
        name,
        b"event create",
    );

    assert_ok(
        unsafe { zx_object_wait_async(event, port, 42, 1 << 24, 0) },
        name,
        b"wait_async",
    );
    assert_ok(unsafe { zx_port_cancel(port, event, 42) }, name, b"cancel");

    unsafe { zx_handle_close(event) };
    unsafe { zx_handle_close(port) };
    pass(name);
}

/// Wait async with bad handle
fn wait_async_bad_handle() {
    let name = b"wait_async_bad_handle";
    let mut port: HandleValue = 0;
    assert_ok(
        unsafe { zx_port_create(0, &mut port) },
        name,
        b"port create",
    );

    let s = unsafe { zx_object_wait_async(0xDEAD, port, 0, 1 << 24, 0) };
    if s != ZX_ERR_BAD_HANDLE {
        fail(name, b"expected BAD_HANDLE");
    }

    unsafe { zx_handle_close(port) };
    pass(name);
}

/// Wait async with wrong type port
fn wait_async_wrong_type_port() {
    let name = b"wait_async_wrong_type_port";
    let mut event1: HandleValue = 0;
    let mut event2: HandleValue = 0;
    assert_ok(unsafe { zx_event_create(0, &mut event1) }, name, b"event1");
    assert_ok(unsafe { zx_event_create(0, &mut event2) }, name, b"event2");

    // Use event as port -> WRONG_TYPE
    let s = unsafe { zx_object_wait_async(event1, event2, 0, 1 << 24, 0) };
    if s != ZX_ERR_WRONG_TYPE {
        fail(name, b"expected WRONG_TYPE");
    }

    unsafe { zx_handle_close(event1) };
    unsafe { zx_handle_close(event2) };
    pass(name);
}

#[no_mangle]
pub fn main() {
    zx::debug_write(b"obj_wait_extra_test: starting\n");
    wait_async_cancel();
    wait_async_bad_handle();
    wait_async_wrong_type_port();
    zx::debug_write(b"obj_wait_extra_test: all tests passed\n");
}
