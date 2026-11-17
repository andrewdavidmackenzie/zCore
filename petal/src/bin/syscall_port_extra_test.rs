//! petal port extra test -- create, queue, cancel with bad handles.
//!
//! Port of zircon-tests/src/syscall_port_extra.rs.

#![no_std]
#![no_main]
#![allow(dead_code)]

extern crate petal;

use zx::sys::*;

// PortPacket is 48 bytes
const PORT_PACKET_SIZE: usize = 48;

fn pass(name: &[u8]) {
    zx::debug_write(b"  port_extra_test: ");
    zx::debug_write(name);
    zx::debug_write(b" PASS\n");
}

fn fail(name: &[u8], msg: &[u8]) {
    zx::debug_write(b"FAIL: port_extra_test: ");
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

/// Create port with BIND_TO_INTERRUPT option
fn port_create_bind_to_interrupt() {
    let name = b"port_create_bind_to_interrupt";
    let mut h: HandleValue = 0;
    assert_ok(unsafe { zx_port_create(1, &mut h) }, name, b"");
    if h == 0 {
        fail(name, b"handle is 0");
    }
    unsafe { zx_handle_close(h) };
    pass(name);
}

/// Queue a user packet
fn port_queue_user_packet() {
    let name = b"port_queue_user_packet";
    let mut h: HandleValue = 0;
    assert_ok(unsafe { zx_port_create(0, &mut h) }, name, b"create");

    let packet = [0u8; PORT_PACKET_SIZE];
    assert_ok(unsafe { zx_port_queue(h, packet.as_ptr()) }, name, b"queue");

    unsafe { zx_handle_close(h) };
    pass(name);
}

/// Queue bad handle
fn port_queue_bad_handle() {
    let name = b"port_queue_bad_handle";
    let packet = [0u8; PORT_PACKET_SIZE];
    assert_status(
        unsafe { zx_port_queue(0xDEAD, packet.as_ptr()) },
        ZX_ERR_BAD_HANDLE,
        name,
        b"",
    );
    pass(name);
}

/// Wait bad handle
fn port_wait_bad_handle() {
    let name = b"port_wait_bad_handle";
    let mut packet = [0u8; PORT_PACKET_SIZE];
    assert_status(
        unsafe { zx_port_wait(0xDEAD, 0, packet.as_mut_ptr()) },
        ZX_ERR_BAD_HANDLE,
        name,
        b"",
    );
    pass(name);
}

/// Cancel bad handle
fn port_cancel_bad_handle() {
    let name = b"port_cancel_bad_handle";
    let mut event: HandleValue = 0;
    assert_ok(
        unsafe { zx_event_create(0, &mut event) },
        name,
        b"event create",
    );
    assert_status(
        unsafe { zx_port_cancel(0xDEAD, event, 0) },
        ZX_ERR_BAD_HANDLE,
        name,
        b"",
    );
    unsafe { zx_handle_close(event) };
    pass(name);
}

/// Cancel key bad handle
fn port_cancel_key_bad_handle() {
    let name = b"port_cancel_key_bad_handle";
    assert_status(
        unsafe { zx_port_cancel_key(0xDEAD, 0, 0) },
        ZX_ERR_BAD_HANDLE,
        name,
        b"",
    );
    pass(name);
}

#[no_mangle]
pub fn main() {
    zx::debug_write(b"port_extra_test: starting\n");

    port_create_bind_to_interrupt();
    port_queue_user_packet();
    port_queue_bad_handle();
    port_wait_bad_handle();
    port_cancel_bad_handle();
    port_cancel_key_bad_handle();

    zx::debug_write(b"port_extra_test: all tests passed\n");
}
