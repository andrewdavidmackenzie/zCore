//! petal port syscall test -- exercises port create/queue/cancel/wait_async.
//!
//! Port of zircon-tests/src/core_tests/syscall_port.rs.

#![no_std]
#![no_main]
#![allow(dead_code)]

extern crate petal;

use zx::sys::*;

// Signal bits
const ZX_EVENT_SIGNALED: u32 = 1 << 3;

// PortPacket is 48 bytes (matches zx_port_packet_t)
const PORT_PACKET_SIZE: usize = 48;

fn pass(name: &[u8]) {
    zx::debug_write(b"  syscall_port_test: ");
    zx::debug_write(name);
    zx::debug_write(b" PASS\n");
}

fn fail(name: &[u8], msg: &[u8]) {
    zx::debug_write(b"FAIL: syscall_port_test: ");
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

fn port_create() -> HandleValue {
    let mut h: HandleValue = 0;
    let s = unsafe { zx_port_create(0, &mut h) };
    if s != ZX_OK {
        fail(b"port_create", b"failed");
    }
    h
}

/// C++: TEST(PortTest, CreateInvalidOptions)
fn port_create_invalid_options() {
    let name = b"port_create_invalid_options";
    let mut h: HandleValue = 0;
    assert_status(
        unsafe { zx_port_create(0xFF, &mut h) },
        ZX_ERR_INVALID_ARGS,
        name,
        b"",
    );
    pass(name);
}

/// C++: TEST(PortTest, QueueWrongType)
fn port_queue_wrong_type() {
    let name = b"port_queue_wrong_type";
    // Create a channel (not a port)
    let mut ch0: HandleValue = 0;
    let mut ch1: HandleValue = 0;
    assert_ok(
        unsafe { zx_channel_create(0, &mut ch0, &mut ch1) },
        name,
        b"channel create",
    );

    let packet = [0u8; PORT_PACKET_SIZE];
    assert_status(
        unsafe { zx_port_queue(ch0, packet.as_ptr()) },
        ZX_ERR_WRONG_TYPE,
        name,
        b"",
    );

    unsafe { zx_handle_close(ch0) };
    unsafe { zx_handle_close(ch1) };
    pass(name);
}

/// C++: TEST(PortTest, QueueAccessDenied)
fn port_queue_access_denied() {
    let name = b"port_queue_access_denied";
    let port_h = port_create();

    // Reduce rights via replace, dropping WRITE — zx_port_queue
    // requires it (the C++ test replaces with
    // ZX_DEFAULT_PORT_RIGHTS & ~ZX_RIGHT_WRITE). The mask must only
    // request rights the port actually has: default port rights —
    // (BASIC & !WAIT) | IO — include DUPLICATE but not WAIT.
    let mut read_only: HandleValue = 0;
    assert_ok(
        unsafe { zx_handle_replace(port_h, ZX_RIGHT_READ | ZX_RIGHT_INSPECT, &mut read_only) },
        name,
        b"replace read-only",
    );

    let packet = [0u8; PORT_PACKET_SIZE];
    assert_status(
        unsafe { zx_port_queue(read_only, packet.as_ptr()) },
        ZX_ERR_ACCESS_DENIED,
        name,
        b"",
    );

    unsafe { zx_handle_close(read_only) };
    pass(name);
}

/// C++: TEST(PortTest, CancelBadSourceHandle)
fn port_cancel_bad_source() {
    let name = b"port_cancel_bad_source";
    let port_h = port_create();
    let s = unsafe { zx_port_cancel(port_h, 0xDEAD_BEEF, 0) };
    if s == ZX_OK {
        fail(name, b"should fail with bad source handle");
    }
    unsafe { zx_handle_close(port_h) };
    pass(name);
}

/// C++: TEST(PortTest, CancelKeyWrongType)
fn port_cancel_key_wrong_type() {
    let name = b"port_cancel_key_wrong_type";
    // Use channel as "port"
    let mut ch0: HandleValue = 0;
    let mut ch1: HandleValue = 0;
    assert_ok(
        unsafe { zx_channel_create(0, &mut ch0, &mut ch1) },
        name,
        b"channel create",
    );

    assert_status(
        unsafe { zx_port_cancel_key(ch0, 0, 0) },
        ZX_ERR_WRONG_TYPE,
        name,
        b"",
    );

    unsafe { zx_handle_close(ch0) };
    unsafe { zx_handle_close(ch1) };
    pass(name);
}

/// C++: TEST(PortTest, QueueNullPtrReturnsInvalidArgs)
fn port_queue_null_ptr() {
    let name = b"port_queue_null_ptr";
    let port_h = port_create();
    let s = unsafe { zx_port_queue(port_h, core::ptr::null()) };
    if s == ZX_OK {
        fail(name, b"null packet should fail");
    }
    unsafe { zx_handle_close(port_h) };
    pass(name);
}

/// C++: TEST(PortTest, AsyncWaitInvalidOption)
fn port_async_wait_invalid_option() {
    let name = b"port_async_wait_invalid_option";
    let port_h = port_create();
    let mut event_h: HandleValue = 0;
    assert_ok(
        unsafe { zx_event_create(0, &mut event_h) },
        name,
        b"event create",
    );

    assert_status(
        unsafe { zx_object_wait_async(event_h, port_h, 0, ZX_EVENT_SIGNALED, 0xFF) },
        ZX_ERR_INVALID_ARGS,
        name,
        b"",
    );

    unsafe { zx_handle_close(event_h) };
    unsafe { zx_handle_close(port_h) };
    pass(name);
}

/// C++: TEST(PortTest, CancelWrongType)
fn port_cancel_wrong_type() {
    let name = b"port_cancel_wrong_type";
    // Use channel as "port"
    let mut ch0: HandleValue = 0;
    let mut ch1: HandleValue = 0;
    assert_ok(
        unsafe { zx_channel_create(0, &mut ch0, &mut ch1) },
        name,
        b"channel create",
    );

    let mut event_h: HandleValue = 0;
    assert_ok(
        unsafe { zx_event_create(0, &mut event_h) },
        name,
        b"event create",
    );

    assert_status(
        unsafe { zx_port_cancel(ch0, event_h, 0) },
        ZX_ERR_WRONG_TYPE,
        name,
        b"",
    );

    unsafe { zx_handle_close(ch0) };
    unsafe { zx_handle_close(ch1) };
    unsafe { zx_handle_close(event_h) };
    pass(name);
}

/// C++: TEST(PortTest, CancelAccessDenied)
fn port_cancel_access_denied() {
    let name = b"port_cancel_access_denied";
    let port_h = port_create();

    // Reduce rights via replace, dropping WRITE — zx_port_cancel
    // requires it (the C++ test drops ZX_RIGHT_WRITE from the port).
    let mut read_only: HandleValue = 0;
    assert_ok(
        unsafe { zx_handle_replace(port_h, ZX_RIGHT_READ | ZX_RIGHT_INSPECT, &mut read_only) },
        name,
        b"replace read-only",
    );

    let mut event_h: HandleValue = 0;
    assert_ok(
        unsafe { zx_event_create(0, &mut event_h) },
        name,
        b"event create",
    );

    assert_status(
        unsafe { zx_port_cancel(read_only, event_h, 0) },
        ZX_ERR_ACCESS_DENIED,
        name,
        b"",
    );

    unsafe { zx_handle_close(read_only) };
    unsafe { zx_handle_close(event_h) };
    pass(name);
}

/// C++: TEST(PortTest, WaitAsyncPortAccessDenied)
fn port_wait_async_port_access_denied() {
    let name = b"port_wait_async_port_access_denied";
    let port_h = port_create();

    // Reduce rights via replace, dropping WRITE — wait_async queues
    // packets into the port, which requires WRITE on it (the C++ test
    // drops ZX_RIGHT_WRITE from the port).
    let mut read_only: HandleValue = 0;
    assert_ok(
        unsafe { zx_handle_replace(port_h, ZX_RIGHT_READ | ZX_RIGHT_INSPECT, &mut read_only) },
        name,
        b"replace read-only port",
    );

    let mut event_h: HandleValue = 0;
    assert_ok(
        unsafe { zx_event_create(0, &mut event_h) },
        name,
        b"event create",
    );

    assert_status(
        unsafe { zx_object_wait_async(event_h, read_only, 0, ZX_EVENT_SIGNALED, 0) },
        ZX_ERR_ACCESS_DENIED,
        name,
        b"",
    );

    unsafe { zx_handle_close(read_only) };
    unsafe { zx_handle_close(event_h) };
    pass(name);
}

/// C++: TEST(PortTest, WaitAsyncPortWrongType)
fn port_wait_async_port_wrong_type() {
    let name = b"port_wait_async_port_wrong_type";
    let mut ch0: HandleValue = 0;
    let mut ch1: HandleValue = 0;
    assert_ok(
        unsafe { zx_channel_create(0, &mut ch0, &mut ch1) },
        name,
        b"channel create",
    );

    let mut event_h: HandleValue = 0;
    assert_ok(
        unsafe { zx_event_create(0, &mut event_h) },
        name,
        b"event create",
    );

    assert_status(
        unsafe { zx_object_wait_async(event_h, ch0, 0, ZX_EVENT_SIGNALED, 0) },
        ZX_ERR_WRONG_TYPE,
        name,
        b"",
    );

    unsafe { zx_handle_close(ch0) };
    unsafe { zx_handle_close(ch1) };
    unsafe { zx_handle_close(event_h) };
    pass(name);
}

/// C++: TEST(PortTest, WaitAsyncTargetAccessDenied)
fn port_wait_async_target_access_denied() {
    let name = b"port_wait_async_target_access_denied";
    let port_h = port_create();

    let mut event_h: HandleValue = 0;
    assert_ok(
        unsafe { zx_event_create(0, &mut event_h) },
        name,
        b"event create",
    );

    // Dup event with SIGNAL only (no WAIT)
    let mut no_wait: HandleValue = 0;
    assert_ok(
        unsafe { zx_handle_duplicate(event_h, ZX_RIGHT_SIGNAL | ZX_RIGHT_INSPECT, &mut no_wait) },
        name,
        b"dup no-wait event",
    );

    assert_status(
        unsafe { zx_object_wait_async(no_wait, port_h, 0, ZX_EVENT_SIGNALED, 0) },
        ZX_ERR_ACCESS_DENIED,
        name,
        b"",
    );

    unsafe { zx_handle_close(no_wait) };
    unsafe { zx_handle_close(event_h) };
    unsafe { zx_handle_close(port_h) };
    pass(name);
}

#[no_mangle]
pub fn main() {
    zx::debug_write(b"syscall_port_test: starting\n");

    port_create_invalid_options();
    port_queue_wrong_type();
    port_queue_access_denied();
    port_cancel_bad_source();
    port_cancel_key_wrong_type();
    port_queue_null_ptr();
    port_async_wait_invalid_option();
    port_cancel_wrong_type();
    port_cancel_access_denied();
    port_wait_async_port_access_denied();
    port_wait_async_port_wrong_type();
    port_wait_async_target_access_denied();

    zx::debug_write(b"syscall_port_test: all tests passed\n");
}
