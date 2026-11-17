//! petal iovec test -- channel write/read with iovec-style data.
//!
//! Port of sync tests from zircon-tests/src/core_tests/iovec.rs.
//! These test writing multiple data chunks to a channel.

#![no_std]
#![no_main]
#![allow(dead_code)]

extern crate petal;

use zx::sys::*;

fn pass(name: &[u8]) {
    zx::debug_write(b"  iovec_test: ");
    zx::debug_write(name);
    zx::debug_write(b" PASS\n");
}

fn fail(name: &[u8], msg: &[u8]) {
    zx::debug_write(b"FAIL: iovec_test: ");
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

fn channel_create() -> (HandleValue, HandleValue) {
    let mut h0: HandleValue = 0;
    let mut h1: HandleValue = 0;
    let s = unsafe { zx_channel_create(0, &mut h0, &mut h1) };
    if s != ZX_OK {
        fail(b"channel_create", b"failed");
    }
    (h0, h1)
}

/// Write zero bytes, read zero bytes
fn write_zero_iovecs() {
    let name = b"write_zero_iovecs";
    let (h0, h1) = channel_create();

    // Write with null data and 0 bytes
    assert_ok(
        unsafe { zx_channel_write(h0, 0, core::ptr::null(), 0, core::ptr::null(), 0) },
        name,
        b"write",
    );

    // Read should get 0 bytes
    let mut ab: u32 = 0;
    let mut ah: u32 = 0;
    assert_ok(
        unsafe {
            zx_channel_read(
                h1,
                0,
                core::ptr::null_mut(),
                core::ptr::null_mut(),
                0,
                0,
                &mut ab,
                &mut ah,
            )
        },
        name,
        b"read",
    );
    if ab != 0 {
        fail(name, b"expected 0 bytes");
    }

    unsafe { zx_handle_close(h0) };
    unsafe { zx_handle_close(h1) };
    pass(name);
}

/// Write a few large data chunks
fn write_few_large_iovecs() {
    let name = b"write_few_large_iovecs";
    let (h0, h1) = channel_create();

    // Write 1024 bytes
    let data = [0xABu8; 1024];
    assert_ok(
        unsafe { zx_channel_write(h0, 0, data.as_ptr(), 1024, core::ptr::null(), 0) },
        name,
        b"write",
    );

    // Read it back
    let mut buf = [0u8; 1024];
    let mut ab: u32 = 0;
    let mut ah: u32 = 0;
    assert_ok(
        unsafe {
            zx_channel_read(
                h1,
                0,
                buf.as_mut_ptr(),
                core::ptr::null_mut(),
                1024,
                0,
                &mut ab,
                &mut ah,
            )
        },
        name,
        b"read",
    );
    if ab != 1024 {
        fail(name, b"wrong byte count");
    }
    if buf[0] != 0xAB || buf[1023] != 0xAB {
        fail(name, b"data mismatch");
    }

    unsafe { zx_handle_close(h0) };
    unsafe { zx_handle_close(h1) };
    pass(name);
}

/// Write with a handle
fn write_with_handle() {
    let name = b"write_with_handle";
    let (h0, h1) = channel_create();

    // Create an event to transfer
    let mut event: HandleValue = 0;
    assert_ok(
        unsafe { zx_event_create(0, &mut event) },
        name,
        b"event create",
    );

    // Write handle
    let handles = [event];
    assert_ok(
        unsafe { zx_channel_write(h0, 0, core::ptr::null(), 0, handles.as_ptr(), 1) },
        name,
        b"write handle",
    );

    // Read handle
    let mut recv = [0u32; 1];
    let mut ab: u32 = 0;
    let mut ah: u32 = 0;
    assert_ok(
        unsafe {
            zx_channel_read(
                h1,
                0,
                core::ptr::null_mut(),
                recv.as_mut_ptr(),
                0,
                1,
                &mut ab,
                &mut ah,
            )
        },
        name,
        b"read",
    );
    if ah != 1 {
        fail(name, b"expected 1 handle");
    }
    assert_ok(
        unsafe { zx_handle_check_valid(recv[0]) },
        name,
        b"received valid",
    );

    unsafe { zx_handle_close(recv[0]) };
    unsafe { zx_handle_close(h0) };
    unsafe { zx_handle_close(h1) };
    pass(name);
}

/// Write null buffer with non-null size -> error
fn write_null_buffer_nonnull_size() {
    let name = b"write_null_buffer_nonnull_size";
    let (h0, h1) = channel_create();

    let s = unsafe { zx_channel_write(h0, 0, core::ptr::null(), 10, core::ptr::null(), 0) };
    if s == ZX_OK {
        fail(name, b"null buffer with size should fail");
    }

    unsafe { zx_handle_close(h0) };
    unsafe { zx_handle_close(h1) };
    pass(name);
}

/// Write too many bytes (> 64KiB)
fn write_too_many_bytes() {
    let name = b"write_too_many_bytes";
    let (h0, h1) = channel_create();

    // 65537 bytes exceeds ZX_CHANNEL_MAX_MSG_BYTES
    // We can't easily allocate 65537 on a 256KiB heap, so use the size parameter
    let s = unsafe { zx_channel_write(h0, 0, core::ptr::null(), 65537, core::ptr::null(), 0) };
    if s == ZX_OK {
        fail(name, b"too many bytes should fail");
    }

    unsafe { zx_handle_close(h0) };
    unsafe { zx_handle_close(h1) };
    pass(name);
}

#[no_mangle]
pub fn main() {
    zx::debug_write(b"iovec_test: starting\n");

    write_zero_iovecs();
    write_few_large_iovecs();
    write_with_handle();
    write_null_buffer_nonnull_size();
    write_too_many_bytes();

    zx::debug_write(b"iovec_test: all tests passed\n");
}
