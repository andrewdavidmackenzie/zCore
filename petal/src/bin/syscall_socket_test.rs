//! petal socket syscall test -- exercises socket create/read/write/disposition.
//!
//! Port of zircon-tests/src/core_tests/syscall_socket.rs.

#![no_std]
#![no_main]
#![allow(dead_code)]

extern crate petal;

use zx::sys::*;

// Socket options
const ZX_SOCKET_STREAM: u32 = 0;
const ZX_SOCKET_DATAGRAM: u32 = 1;

// Socket properties
const ZX_PROP_SOCKET_RX_THRESHOLD: u32 = 12;
const ZX_PROP_SOCKET_TX_THRESHOLD: u32 = 13;

fn pass(name: &[u8]) {
    zx::debug_write(b"  syscall_socket_test: ");
    zx::debug_write(name);
    zx::debug_write(b" PASS\n");
}

fn fail(name: &[u8], msg: &[u8]) {
    zx::debug_write(b"FAIL: syscall_socket_test: ");
    zx::debug_write(name);
    zx::debug_write(b" - ");
    zx::debug_write(msg);
    zx::debug_write(b"\n");
    zx::Process::exit(1);
}

fn write_i32(val: i32) {
    if val < 0 {
        zx::debug_write(b"-");
        let mut v = (-val) as u32;
        let mut buf = [0u8; 10];
        let mut i = buf.len();
        while v > 0 {
            i -= 1;
            buf[i] = b'0' + (v % 10) as u8;
            v /= 10;
        }
        if i == buf.len() {
            zx::debug_write(b"0");
        } else {
            zx::debug_write(&buf[i..]);
        }
    } else {
        let mut v = val as u32;
        let mut buf = [0u8; 10];
        let mut i = buf.len();
        while v > 0 {
            i -= 1;
            buf[i] = b'0' + (v % 10) as u8;
            v /= 10;
        }
        if i == buf.len() {
            zx::debug_write(b"0");
        } else {
            zx::debug_write(&buf[i..]);
        }
    }
}

fn assert_ok(s: ZxStatus, name: &[u8], msg: &[u8]) {
    if s != ZX_OK {
        zx::debug_write(b"  status=");
        write_i32(s);
        zx::debug_write(b"\n");
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

/// C++: TEST(SocketTest, WriteRights)
fn write_without_write_right() {
    let name = b"write_without_write_right";
    let (h0, h1) = socket_create(ZX_SOCKET_STREAM);

    // Dup with READ only (no WRITE)
    let mut read_only: HandleValue = 0;
    assert_ok(
        unsafe {
            zx_handle_duplicate(
                h0,
                ZX_RIGHT_READ | ZX_RIGHT_WAIT | ZX_RIGHT_INSPECT,
                &mut read_only,
            )
        },
        name,
        b"dup",
    );

    let mut actual: usize = 0;
    assert_status(
        unsafe { zx_socket_write(read_only, 0, b"test".as_ptr(), 4, &mut actual) },
        ZX_ERR_ACCESS_DENIED,
        name,
        b"",
    );

    unsafe { zx_handle_close(read_only) };
    unsafe { zx_handle_close(h0) };
    unsafe { zx_handle_close(h1) };
    pass(name);
}

/// C++: TEST(SocketTest, ReadRights)
fn read_without_read_right() {
    let name = b"read_without_read_right";
    let (h0, h1) = socket_create(ZX_SOCKET_STREAM);

    // Dup with WRITE only (no READ)
    let mut write_only: HandleValue = 0;
    assert_ok(
        unsafe {
            zx_handle_duplicate(
                h0,
                ZX_RIGHT_WRITE | ZX_RIGHT_WAIT | ZX_RIGHT_INSPECT,
                &mut write_only,
            )
        },
        name,
        b"dup",
    );

    let mut buf = [0u8; 16];
    let mut actual: usize = 0;
    assert_status(
        unsafe { zx_socket_read(write_only, 0, buf.as_mut_ptr(), buf.len(), &mut actual) },
        ZX_ERR_ACCESS_DENIED,
        name,
        b"",
    );

    unsafe { zx_handle_close(write_only) };
    unsafe { zx_handle_close(h0) };
    unsafe { zx_handle_close(h1) };
    pass(name);
}

/// C++: TEST(SocketTest, WriteWrongType)
fn write_wrong_type() {
    let name = b"write_wrong_type";
    // Create a VMO (not a socket)
    let mut vmo_h: HandleValue = 0;
    assert_ok(
        unsafe { zx_vmo_create(4096, 0, &mut vmo_h) },
        name,
        b"vmo create",
    );

    let mut actual: usize = 0;
    assert_status(
        unsafe { zx_socket_write(vmo_h, 0, b"test".as_ptr(), 4, &mut actual) },
        ZX_ERR_WRONG_TYPE,
        name,
        b"",
    );

    unsafe { zx_handle_close(vmo_h) };
    pass(name);
}

/// C++: TEST(SocketTest, ReadWrongType)
fn read_wrong_type() {
    let name = b"read_wrong_type";
    let mut vmo_h: HandleValue = 0;
    assert_ok(
        unsafe { zx_vmo_create(4096, 0, &mut vmo_h) },
        name,
        b"vmo create",
    );

    let mut buf = [0u8; 16];
    let mut actual: usize = 0;
    assert_status(
        unsafe { zx_socket_read(vmo_h, 0, buf.as_mut_ptr(), buf.len(), &mut actual) },
        ZX_ERR_WRONG_TYPE,
        name,
        b"",
    );

    unsafe { zx_handle_close(vmo_h) };
    pass(name);
}

/// C++: TEST(SocketTest, InvalidHandles)
fn invalid_handles() {
    let name = b"invalid_handles";

    let mut actual: usize = 0;
    assert_status(
        unsafe { zx_socket_write(0xDEAD, 0, b"x".as_ptr(), 1, &mut actual) },
        ZX_ERR_BAD_HANDLE,
        name,
        b"write bad handle",
    );
    let mut buf = [0u8; 1];
    assert_status(
        unsafe { zx_socket_read(0xDEAD, 0, buf.as_mut_ptr(), 1, &mut actual) },
        ZX_ERR_BAD_HANDLE,
        name,
        b"read bad handle",
    );

    pass(name);
}

/// C++: TEST(SocketTest, WriteFromBadBuffer)
fn write_from_bad_buffer() {
    let name = b"write_from_bad_buffer";
    let (h0, h1) = socket_create(ZX_SOCKET_STREAM);

    let bad_ptr = 4096 as *const u8; // unmapped address
    let mut actual: usize = 0;
    let s = unsafe { zx_socket_write(h0, 0, bad_ptr, 1, &mut actual) };
    if s == ZX_OK {
        fail(name, b"write from bad buffer should fail");
    }

    unsafe { zx_handle_close(h0) };
    unsafe { zx_handle_close(h1) };
    pass(name);
}

/// C++: TEST(SocketTest, WriteFromNullBuffer)
fn write_from_null_buffer() {
    let name = b"write_from_null_buffer";
    let (h0, h1) = socket_create(ZX_SOCKET_STREAM);

    let mut actual: usize = 0;
    let s = unsafe { zx_socket_write(h0, 0, core::ptr::null(), 1, &mut actual) };
    if s == ZX_OK {
        fail(name, b"write from null should fail");
    }

    unsafe { zx_handle_close(h0) };
    unsafe { zx_handle_close(h1) };
    pass(name);
}

/// C++: TEST(SocketTest, ReadIntoNullBuffer)
fn read_into_null_buffer() {
    let name = b"read_into_null_buffer";
    let (h0, h1) = socket_create(ZX_SOCKET_STREAM);

    // Write some data first
    let mut actual: usize = 0;
    assert_ok(
        unsafe { zx_socket_write(h0, 0, b"data".as_ptr(), 4, &mut actual) },
        name,
        b"write",
    );

    // Read into null
    let s = unsafe { zx_socket_read(h1, 0, core::ptr::null_mut(), 4, &mut actual) };
    if s == ZX_OK {
        fail(name, b"read into null should fail");
    }

    unsafe { zx_handle_close(h0) };
    unsafe { zx_handle_close(h1) };
    pass(name);
}

/// C++: TEST(SocketTest, GetPropertyRxThresholdWrongType)
fn get_property_rx_threshold_wrong_type() {
    let name = b"get_property_rx_threshold_wrong_type";
    // Use a VMO like the C++ original: VMOs carry GET_PROPERTY rights,
    // so the failure comes from the type check (WRONG_TYPE), not rights.
    let mut vmo_h: HandleValue = 0;
    assert_ok(
        unsafe { zx_vmo_create(4096, 0, &mut vmo_h) },
        name,
        b"vmo create",
    );

    let mut val: usize = 0;
    assert_status(
        unsafe {
            zx_object_get_property(
                vmo_h,
                ZX_PROP_SOCKET_RX_THRESHOLD,
                &mut val as *mut _ as *mut u8,
                core::mem::size_of::<usize>(),
            )
        },
        ZX_ERR_WRONG_TYPE,
        name,
        b"",
    );

    unsafe { zx_handle_close(vmo_h) };
    pass(name);
}

/// C++: TEST(SocketTest, SetPropertyRxThresholdWrongType)
fn set_property_rx_threshold_wrong_type() {
    let name = b"set_property_rx_threshold_wrong_type";
    // Use a VMO like the C++ original: VMOs carry SET_PROPERTY rights,
    // so the failure comes from the type check (WRONG_TYPE), not rights.
    let mut vmo_h: HandleValue = 0;
    assert_ok(
        unsafe { zx_vmo_create(4096, 0, &mut vmo_h) },
        name,
        b"vmo create",
    );

    let val: usize = 0;
    assert_status(
        unsafe {
            zx_object_set_property(
                vmo_h,
                ZX_PROP_SOCKET_RX_THRESHOLD,
                &val as *const _ as *const u8,
                core::mem::size_of::<usize>(),
            )
        },
        ZX_ERR_WRONG_TYPE,
        name,
        b"",
    );

    unsafe { zx_handle_close(vmo_h) };
    pass(name);
}

/// C++: TEST(SocketTest, DisableWriteLocal)
fn disable_write_local() {
    let name = b"disable_write_local";
    let (h0, h1) = socket_create(ZX_SOCKET_STREAM);

    // Disable write on the local endpoint: the `disposition` argument
    // controls THIS socket's writes (disposition_peer controls the peer's).
    assert_ok(
        unsafe {
            zx_socket_set_disposition(h0, 1 /* ZX_SOCKET_DISPOSITION_WRITE_DISABLED */, 0)
        },
        name,
        b"disable write",
    );

    // Write should fail with BAD_STATE
    let mut actual: usize = 0;
    assert_status(
        unsafe { zx_socket_write(h0, 0, b"test".as_ptr(), 4, &mut actual) },
        ZX_ERR_BAD_STATE,
        name,
        b"write after disable",
    );

    unsafe { zx_handle_close(h0) };
    unsafe { zx_handle_close(h1) };
    pass(name);
}

#[no_mangle]
pub fn main() {
    zx::debug_write(b"syscall_socket_test: starting\n");

    write_without_write_right();
    read_without_read_right();
    write_wrong_type();
    read_wrong_type();
    invalid_handles();
    write_from_bad_buffer();
    write_from_null_buffer();
    read_into_null_buffer();
    get_property_rx_threshold_wrong_type();
    set_property_rx_threshold_wrong_type();
    disable_write_local();

    zx::debug_write(b"syscall_socket_test: all tests passed\n");
}
