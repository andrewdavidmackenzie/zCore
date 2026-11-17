//! petal socket test -- comprehensive socket syscall tests from user mode.
//!
//! Port of zircon-tests/src/core_tests/socket.rs.

#![no_std]
#![no_main]
#![allow(dead_code)]

extern crate alloc;
extern crate petal;

use alloc::vec;
use zx::sys::*;

const ZX_INFO_HANDLE_BASIC: u32 = 2;
const ZX_INFO_SOCKET: u32 = 15;
const ZX_PROP_SOCKET_RX_THRESHOLD: u32 = 12;
const ZX_PROP_SOCKET_TX_THRESHOLD: u32 = 13;

fn pass(name: &[u8]) {
    zx::debug_write(b"  socket_test: ");
    zx::debug_write(name);
    zx::debug_write(b" PASS\n");
}

fn fail(name: &[u8], msg: &[u8]) {
    zx::debug_write(b"FAIL: socket_test: ");
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

fn socket_create(options: u32) -> (HandleValue, HandleValue) {
    let mut h0: HandleValue = 0;
    let mut h1: HandleValue = 0;
    let s = unsafe { zx_socket_create(options, &mut h0, &mut h1) };
    if s != ZX_OK {
        fail(b"socket_create", b"failed");
    }
    (h0, h1)
}

/// Empty socket read -> SHOULD_WAIT
fn empty_socket_should_wait() {
    let name = b"empty_socket_should_wait";
    let (h0, h1) = socket_create(0);
    let mut buf = [0u8; 16];
    let mut actual: usize = 0;
    assert_status(
        unsafe { zx_socket_read(h1, 0, buf.as_mut_ptr(), buf.len(), &mut actual) },
        ZX_ERR_SHOULD_WAIT,
        name,
        b"",
    );
    unsafe { zx_handle_close(h0) };
    unsafe { zx_handle_close(h1) };
    pass(name);
}

/// Write then read data, verify content
fn write_read_data_verify() {
    let name = b"write_read_data_verify";
    let (h0, h1) = socket_create(0);

    let data = b"hello socket";
    let mut actual: usize = 0;
    assert_ok(
        unsafe { zx_socket_write(h0, 0, data.as_ptr(), data.len(), &mut actual) },
        name,
        b"write",
    );
    if actual != data.len() {
        fail(name, b"write actual wrong");
    }

    let mut buf = [0u8; 32];
    assert_ok(
        unsafe { zx_socket_read(h1, 0, buf.as_mut_ptr(), buf.len(), &mut actual) },
        name,
        b"read",
    );
    if actual != data.len() {
        fail(name, b"read actual wrong");
    }
    if &buf[..data.len()] != data {
        fail(name, b"data mismatch");
    }

    unsafe { zx_handle_close(h0) };
    unsafe { zx_handle_close(h1) };
    pass(name);
}

/// Peer closed -> PEER_CLOSED on write
fn peer_closed_error() {
    let name = b"peer_closed_error";
    let (h0, h1) = socket_create(0);
    unsafe { zx_handle_close(h1) };

    let data = b"test";
    let mut actual: usize = 0;
    assert_status(
        unsafe { zx_socket_write(h0, 0, data.as_ptr(), data.len(), &mut actual) },
        ZX_ERR_PEER_CLOSED,
        name,
        b"",
    );

    unsafe { zx_handle_close(h0) };
    pass(name);
}

/// Peeking leaves data in socket
fn peeking_leaves_data() {
    let name = b"peeking_leaves_data";
    let (h0, h1) = socket_create(0);

    let data = b"peek";
    let mut actual: usize = 0;
    assert_ok(
        unsafe { zx_socket_write(h0, 0, data.as_ptr(), data.len(), &mut actual) },
        name,
        b"write",
    );

    // Peek (option 8 = ZX_SOCKET_PEEK)
    let mut buf = [0u8; 16];
    assert_ok(
        unsafe { zx_socket_read(h1, 8, buf.as_mut_ptr(), buf.len(), &mut actual) },
        name,
        b"peek",
    );
    if actual != 4 {
        fail(name, b"peek actual wrong");
    }
    if &buf[..4] != b"peek" {
        fail(name, b"peek data wrong");
    }

    // Data should still be there
    assert_ok(
        unsafe { zx_socket_read(h1, 0, buf.as_mut_ptr(), buf.len(), &mut actual) },
        name,
        b"read after peek",
    );
    if actual != 4 {
        fail(name, b"read actual wrong after peek");
    }

    unsafe { zx_handle_close(h0) };
    unsafe { zx_handle_close(h1) };
    pass(name);
}

/// Create with invalid args
fn create_invalid_args() {
    let name = b"create_invalid_args";
    let mut h0: HandleValue = 0;
    let mut h1: HandleValue = 0;
    // Invalid options (not 0 or 1)
    let s = unsafe { zx_socket_create(0xFF, &mut h0, &mut h1) };
    if s == ZX_OK {
        fail(name, b"invalid options should fail");
    }
    pass(name);
}

/// Zero-size write should succeed as no-op
fn zero_size() {
    let name = b"zero_size";
    let (h0, h1) = socket_create(0);

    let mut actual: usize = 0;
    assert_ok(
        unsafe { zx_socket_write(h0, 0, core::ptr::null(), 0, &mut actual) },
        name,
        b"zero write",
    );

    unsafe { zx_handle_close(h0) };
    unsafe { zx_handle_close(h1) };
    pass(name);
}

/// Default rights and type
fn default_rights_and_type() {
    let name = b"default_rights_and_type";
    let (h0, h1) = socket_create(0);

    let mut buf = [0u8; 32];
    let mut actual: usize = 0;
    let mut avail: usize = 0;
    assert_ok(
        unsafe {
            zx_object_get_info(
                h0,
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

    // obj_type at offset 12: Socket = 14
    let obj_type = u32::from_le_bytes(buf[12..16].try_into().unwrap());
    if obj_type != 14 {
        fail(name, b"wrong obj_type (expected 14=Socket)");
    }

    unsafe { zx_handle_close(h0) };
    unsafe { zx_handle_close(h1) };
    pass(name);
}

/// Non-allowed signal bits -> INVALID_ARGS
fn non_allowed_signals() {
    let name = b"non_allowed_signals";
    let (h0, h1) = socket_create(0);

    // Bit 0 (READABLE) is a kernel signal
    assert_status(
        unsafe { zx_object_signal(h0, 0, 1) },
        ZX_ERR_INVALID_ARGS,
        name,
        b"kernel signal",
    );

    unsafe { zx_handle_close(h0) };
    unsafe { zx_handle_close(h1) };
    pass(name);
}

/// Set/get threshold properties
fn set_thresholds_prop() {
    let name = b"set_thresholds_prop";
    let (h0, h1) = socket_create(0);

    // Set RX threshold to 100
    let val: usize = 100;
    assert_ok(
        unsafe {
            zx_object_set_property(
                h0,
                ZX_PROP_SOCKET_RX_THRESHOLD,
                &val as *const _ as *const u8,
                core::mem::size_of::<usize>(),
            )
        },
        name,
        b"set rx threshold",
    );

    // Get RX threshold
    let mut read_val: usize = 0;
    assert_ok(
        unsafe {
            zx_object_get_property(
                h0,
                ZX_PROP_SOCKET_RX_THRESHOLD,
                &mut read_val as *mut _ as *mut u8,
                core::mem::size_of::<usize>(),
            )
        },
        name,
        b"get rx threshold",
    );
    if read_val != 100 {
        fail(name, b"rx threshold mismatch");
    }

    unsafe { zx_handle_close(h0) };
    unsafe { zx_handle_close(h1) };
    pass(name);
}

/// Disposition wrong type
fn set_disposition_wrong_type() {
    let name = b"set_disposition_wrong_type";
    let mut event: HandleValue = 0;
    assert_ok(
        unsafe { zx_event_create(0, &mut event) },
        name,
        b"event create",
    );

    let s = unsafe { zx_socket_set_disposition(event, 0, 0) };
    if s != ZX_ERR_WRONG_TYPE {
        fail(name, b"expected WRONG_TYPE");
    }

    unsafe { zx_handle_close(event) };
    pass(name);
}

/// Disposition without MANAGE_SOCKET right
fn set_disposition_handle_without_right() {
    let name = b"set_disposition_no_right";
    let (h0, h1) = socket_create(0);

    // Dup without MANAGE_SOCKET
    let mut limited: HandleValue = 0;
    assert_ok(
        unsafe {
            zx_handle_duplicate(
                h0,
                ZX_RIGHT_READ | ZX_RIGHT_WRITE | ZX_RIGHT_WAIT | ZX_RIGHT_INSPECT,
                &mut limited,
            )
        },
        name,
        b"dup",
    );

    let s = unsafe { zx_socket_set_disposition(limited, 0, 1) };
    if s != ZX_ERR_ACCESS_DENIED {
        fail(name, b"expected ACCESS_DENIED");
    }

    unsafe { zx_handle_close(limited) };
    unsafe { zx_handle_close(h0) };
    unsafe { zx_handle_close(h1) };
    pass(name);
}

/// Large write/read
fn large_write_read() {
    let name = b"large_write_read";
    let (h0, h1) = socket_create(0);

    let data = vec![0xABu8; 4096];
    let mut actual: usize = 0;
    assert_ok(
        unsafe { zx_socket_write(h0, 0, data.as_ptr(), data.len(), &mut actual) },
        name,
        b"write",
    );
    if actual != 4096 {
        fail(name, b"write actual wrong");
    }

    let mut buf = vec![0u8; 4096];
    assert_ok(
        unsafe { zx_socket_read(h1, 0, buf.as_mut_ptr(), buf.len(), &mut actual) },
        name,
        b"read",
    );
    if actual != 4096 {
        fail(name, b"read actual wrong");
    }
    if buf != data {
        fail(name, b"data mismatch");
    }

    unsafe { zx_handle_close(h0) };
    unsafe { zx_handle_close(h1) };
    pass(name);
}

#[no_mangle]
pub fn main() {
    zx::debug_write(b"socket_test: starting\n");

    empty_socket_should_wait();
    write_read_data_verify();
    peer_closed_error();
    peeking_leaves_data();
    create_invalid_args();
    zero_size();
    default_rights_and_type();
    non_allowed_signals();
    set_thresholds_prop();
    set_disposition_wrong_type();
    set_disposition_handle_without_right();
    large_write_read();

    zx::debug_write(b"socket_test: all tests passed\n");
}
