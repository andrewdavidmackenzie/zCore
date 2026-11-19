//! petal port test -- port queue, cancel, async wait.
//!
//! Port of sync tests from zircon-tests/src/core_tests/port.rs.

#![no_std]
#![no_main]
#![allow(dead_code)]

extern crate petal;

use zx::sys::*;

const PORT_PACKET_SIZE: usize = 48;

fn pass(name: &[u8]) {
    zx::debug_write(b"  port_test: ");
    zx::debug_write(name);
    zx::debug_write(b" PASS\n");
}

fn fail(name: &[u8], msg: &[u8]) {
    zx::debug_write(b"FAIL: port_test: ");
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

/// Queue a user packet and read it back
fn port_queue_and_close() {
    let name = b"port_queue_and_close";
    let mut port: HandleValue = 0;
    assert_ok(unsafe { zx_port_create(0, &mut port) }, name, b"create");

    // Queue a user packet (zeroed)
    let mut packet = [0u8; PORT_PACKET_SIZE];
    // Set key to 42 (u64 at offset 0)
    packet[0..8].copy_from_slice(&42u64.to_ne_bytes());
    // Set type to ZX_PKT_TYPE_USER (0) at offset 8 (u32)
    // Type 0 = user packet

    assert_ok(
        unsafe { zx_port_queue(port, packet.as_ptr()) },
        name,
        b"queue",
    );

    // Wait and read it back (timeout=0, should succeed immediately)
    let mut recv = [0u8; PORT_PACKET_SIZE];
    assert_ok(
        unsafe { zx_port_wait(port, 0, recv.as_mut_ptr()) },
        name,
        b"wait",
    );

    // Check key
    let key = u64::from_ne_bytes(recv[0..8].try_into().unwrap());
    if key != 42 {
        fail(name, b"key mismatch");
    }

    unsafe { zx_handle_close(port) };
    pass(name);
}

/// Cancel key on port
fn cancel_key() {
    let name = b"cancel_key";
    let mut port: HandleValue = 0;
    assert_ok(unsafe { zx_port_create(0, &mut port) }, name, b"create");

    // Cancel with key 0 — should succeed (nothing to cancel, but no error)
    let s = unsafe { zx_port_cancel_key(port, 0, 0) };
    // May return OK or NOT_FOUND depending on implementation
    if s != ZX_OK && s != ZX_ERR_NOT_FOUND {
        fail(name, b"unexpected error");
    }

    unsafe { zx_handle_close(port) };
    pass(name);
}

/// Cancel key access denied
fn cancel_key_access_denied() {
    let name = b"cancel_key_access_denied";
    let mut port: HandleValue = 0;
    assert_ok(unsafe { zx_port_create(0, &mut port) }, name, b"create");

    // Reduce rights via replace, dropping WRITE — zx_port_cancel_key
    // requires it (the C++ Cancel* tests replace with
    // ZX_DEFAULT_PORT_RIGHTS & ~ZX_RIGHT_WRITE). The mask must only
    // request rights the port actually has: default port rights —
    // (BASIC & !WAIT) | IO — include DUPLICATE but not WAIT.
    let mut ro: HandleValue = 0;
    assert_ok(
        unsafe { zx_handle_replace(port, ZX_RIGHT_READ | ZX_RIGHT_INSPECT, &mut ro) },
        name,
        b"replace",
    );

    assert_status(
        unsafe { zx_port_cancel_key(ro, 0, 0) },
        ZX_ERR_ACCESS_DENIED,
        name,
        b"",
    );

    unsafe { zx_handle_close(ro) };
    pass(name);
}

/// Cancel key invalid options
fn cancel_key_invalid_options() {
    let name = b"cancel_key_invalid_options";
    let mut port: HandleValue = 0;
    assert_ok(unsafe { zx_port_create(0, &mut port) }, name, b"create");

    assert_status(
        unsafe { zx_port_cancel_key(port, 0xFF, 0) },
        ZX_ERR_INVALID_ARGS,
        name,
        b"",
    );

    unsafe { zx_handle_close(port) };
    pass(name);
}

/// Async wait on event, then cancel
fn port_edge_trigger_cancel() {
    let name = b"port_edge_trigger_cancel";
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

    // Subscribe to signal on event via port
    assert_ok(
        unsafe {
            zx_object_wait_async(event, port, 42, 1 << 24 /* USER_SIGNAL_0 */, 0)
        },
        name,
        b"wait_async",
    );

    // Cancel the subscription
    assert_ok(unsafe { zx_port_cancel(port, event, 42) }, name, b"cancel");

    unsafe { zx_handle_close(event) };
    unsafe { zx_handle_close(port) };
    pass(name);
}

#[no_mangle]
pub fn main() {
    zx::debug_write(b"port_test: starting\n");

    port_queue_and_close();
    cancel_key();
    cancel_key_access_denied();
    cancel_key_invalid_options();
    port_edge_trigger_cancel();

    zx::debug_write(b"port_test: all tests passed\n");
}
