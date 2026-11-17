//! petal FIFO syscall test -- exercises FIFO create/read/write from user mode.
//!
//! Port of zircon-tests/src/core_tests/syscall_fifo.rs.

#![no_std]
#![no_main]
#![allow(dead_code)]

extern crate petal;

use zx::sys::*;

fn pass(name: &[u8]) {
    zx::debug_write(b"  syscall_fifo_test: ");
    zx::debug_write(name);
    zx::debug_write(b" PASS\n");
}

fn fail(name: &[u8], msg: &[u8]) {
    zx::debug_write(b"FAIL: syscall_fifo_test: ");
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

fn fifo_create(elem_count: usize, elem_size: usize) -> (HandleValue, HandleValue) {
    let mut h0: HandleValue = 0;
    let mut h1: HandleValue = 0;
    let s = unsafe { zx_fifo_create(elem_count, elem_size, 0, &mut h0, &mut h1) };
    if s != ZX_OK {
        fail(b"fifo_create", b"failed");
    }
    (h0, h1)
}

/// C++: TEST(FifoTest, InvalidParametersReturnOutOfRange)
fn invalid_parameters() {
    let name = b"invalid_parameters";
    let mut h0: HandleValue = 0;
    let mut h1: HandleValue = 0;

    // Zero count
    assert_status(
        unsafe { zx_fifo_create(0, 4, 0, &mut h0, &mut h1) },
        ZX_ERR_OUT_OF_RANGE,
        name,
        b"zero count",
    );

    // Zero elem_size
    assert_status(
        unsafe { zx_fifo_create(16, 0, 0, &mut h0, &mut h1) },
        ZX_ERR_OUT_OF_RANGE,
        name,
        b"zero elem_size",
    );

    pass(name);
}

/// C++: TEST(FifoTest, WriteNoWriteRights)
fn write_no_write_rights() {
    let name = b"write_no_write_rights";
    let (h0, h1) = fifo_create(16, 4);

    // Duplicate h0 with READ only (no WRITE)
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
        b"dup read-only",
    );

    let data = [0u8; 4];
    let mut actual: usize = 0;
    assert_status(
        unsafe { zx_fifo_write(read_only, 4, data.as_ptr(), 1, &mut actual) },
        ZX_ERR_ACCESS_DENIED,
        name,
        b"write without WRITE right",
    );

    unsafe { zx_handle_close(read_only) };
    unsafe { zx_handle_close(h0) };
    unsafe { zx_handle_close(h1) };
    pass(name);
}

/// C++: TEST(FifoTest, ReadNoReadRights)
fn read_no_read_rights() {
    let name = b"read_no_read_rights";
    let (h0, h1) = fifo_create(16, 4);

    // Duplicate h0 with WRITE only (no READ)
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
        b"dup write-only",
    );

    let mut buf = [0u8; 4];
    let mut actual: usize = 0;
    assert_status(
        unsafe { zx_fifo_read(write_only, 4, buf.as_mut_ptr(), 1, &mut actual) },
        ZX_ERR_ACCESS_DENIED,
        name,
        b"read without READ right",
    );

    unsafe { zx_handle_close(write_only) };
    unsafe { zx_handle_close(h0) };
    unsafe { zx_handle_close(h1) };
    pass(name);
}

/// C++: TEST(FifoTest, ReadAndWriteValidatesSizeAndElementCount)
fn read_and_write_validates_size_and_element_count() {
    let name = b"read_and_write_validates_size";
    let (h0, h1) = fifo_create(8, 8);

    let data = [0u8; 64];
    let mut buf = [0u8; 64];
    let mut actual: usize = 0;

    // Zero elements
    assert_status(
        unsafe { zx_fifo_read(h1, 8, buf.as_mut_ptr(), 0, &mut actual) },
        ZX_ERR_OUT_OF_RANGE,
        name,
        b"read zero elements",
    );
    assert_status(
        unsafe { zx_fifo_write(h0, 8, data.as_ptr(), 0, &mut actual) },
        ZX_ERR_OUT_OF_RANGE,
        name,
        b"write zero elements",
    );

    // Wrong element size (created with 8, using 9)
    assert_status(
        unsafe { zx_fifo_read(h1, 9, buf.as_mut_ptr(), 8, &mut actual) },
        ZX_ERR_OUT_OF_RANGE,
        name,
        b"read wrong elem_size",
    );
    assert_status(
        unsafe { zx_fifo_write(h0, 9, data.as_ptr(), 8, &mut actual) },
        ZX_ERR_OUT_OF_RANGE,
        name,
        b"write wrong elem_size",
    );

    unsafe { zx_handle_close(h0) };
    unsafe { zx_handle_close(h1) };
    pass(name);
}

/// C++: TEST(FifoTest, ReadWriteNullActualCount)
fn read_write_null_actual_count() {
    let name = b"read_write_null_actual_count";
    let (h0, h1) = fifo_create(8, 8);

    // Write with null actual_count
    let data = 1234u64.to_ne_bytes();
    assert_ok(
        unsafe { zx_fifo_write(h0, 8, data.as_ptr(), 1, core::ptr::null_mut()) },
        name,
        b"write null actual",
    );

    // Read with null actual_count
    let mut buf = [0u8; 8];
    assert_ok(
        unsafe { zx_fifo_read(h1, 8, buf.as_mut_ptr(), 1, core::ptr::null_mut()) },
        name,
        b"read null actual",
    );
    if u64::from_ne_bytes(buf) != 1234 {
        fail(name, b"data mismatch");
    }

    unsafe { zx_handle_close(h0) };
    unsafe { zx_handle_close(h1) };
    pass(name);
}

/// C++: TEST(FifoTest, WriteToClosedPeerReturnsPeerClosed)
fn write_to_closed_peer_returns_peer_closed() {
    let name = b"write_to_closed_peer";
    let (h0, h1) = fifo_create(8, 8);

    // Close the peer
    unsafe { zx_handle_close(h1) };

    let data = [0u8; 8];
    let mut actual: usize = 0;
    assert_status(
        unsafe { zx_fifo_write(h0, 8, data.as_ptr(), 1, &mut actual) },
        ZX_ERR_PEER_CLOSED,
        name,
        b"expected PEER_CLOSED",
    );

    unsafe { zx_handle_close(h0) };
    pass(name);
}

#[no_mangle]
pub fn main() {
    zx::debug_write(b"syscall_fifo_test: starting\n");

    invalid_parameters();
    write_no_write_rights();
    read_no_read_rights();
    read_and_write_validates_size_and_element_count();
    read_write_null_actual_count();
    write_to_closed_peer_returns_peer_closed();

    zx::debug_write(b"syscall_fifo_test: all tests passed\n");
}
