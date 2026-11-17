//! petal channel extra test -- write/read data and handles, rights enforcement.
//!
//! Port of zircon-tests/src/syscall_channel_extra.rs.

#![no_std]
#![no_main]
#![allow(dead_code)]

extern crate alloc;
extern crate petal;

use alloc::vec;
use zx::sys::*;

fn pass(name: &[u8]) {
    zx::debug_write(b"  channel_extra_test: ");
    zx::debug_write(name);
    zx::debug_write(b" PASS\n");
}

fn fail(name: &[u8], msg: &[u8]) {
    zx::debug_write(b"FAIL: channel_extra_test: ");
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

fn channel_create() -> (HandleValue, HandleValue) {
    let mut h0: HandleValue = 0;
    let mut h1: HandleValue = 0;
    let s = unsafe { zx_channel_create(0, &mut h0, &mut h1) };
    if s != ZX_OK {
        fail(b"channel_create", b"failed");
    }
    (h0, h1)
}

/// Write data, read it back
fn write_and_read_via_syscall() {
    let name = b"write_and_read";
    let (h0, h1) = channel_create();

    let data = b"hello";
    assert_ok(
        unsafe {
            zx_channel_write(
                h0,
                0,
                data.as_ptr(),
                data.len() as u32,
                core::ptr::null(),
                0,
            )
        },
        name,
        b"write",
    );

    let mut buf = [0u8; 64];
    let mut actual_bytes: u32 = 0;
    let mut actual_handles: u32 = 0;
    assert_ok(
        unsafe {
            zx_channel_read(
                h1,
                0,
                buf.as_mut_ptr(),
                core::ptr::null_mut(),
                buf.len() as u32,
                0,
                &mut actual_bytes,
                &mut actual_handles,
            )
        },
        name,
        b"read",
    );
    if actual_bytes != 5 {
        fail(name, b"wrong byte count");
    }
    if &buf[..5] != b"hello" {
        fail(name, b"data mismatch");
    }

    unsafe { zx_handle_close(h0) };
    unsafe { zx_handle_close(h1) };
    pass(name);
}

/// Write a handle through channel, read it back
fn write_handle_transfer() {
    let name = b"write_handle_transfer";
    let (ch0, ch1) = channel_create();

    // Create an event to transfer
    let mut event: HandleValue = 0;
    assert_ok(
        unsafe { zx_event_create(0, &mut event) },
        name,
        b"event create",
    );

    // Write event handle through ch0
    let handles = [event];
    assert_ok(
        unsafe { zx_channel_write(ch0, 0, core::ptr::null(), 0, handles.as_ptr(), 1) },
        name,
        b"write handle",
    );

    // Event handle should be consumed (invalid now)
    let s = unsafe { zx_handle_check_valid(event) };
    if s == ZX_OK {
        fail(name, b"event should be consumed");
    }

    // Read handle from ch1
    let mut recv = [0u32; 1];
    let mut ab: u32 = 0;
    let mut ah: u32 = 0;
    assert_ok(
        unsafe {
            zx_channel_read(
                ch1,
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
        b"read handle",
    );
    if ah != 1 {
        fail(name, b"expected 1 handle");
    }
    if recv[0] == 0 {
        fail(name, b"received handle is 0");
    }

    // Received handle should be valid
    assert_ok(
        unsafe { zx_handle_check_valid(recv[0]) },
        name,
        b"received handle valid",
    );

    unsafe { zx_handle_close(recv[0]) };
    unsafe { zx_handle_close(ch0) };
    unsafe { zx_handle_close(ch1) };
    pass(name);
}

/// Write without WRITE right -> ACCESS_DENIED
fn write_without_write_right() {
    let name = b"write_without_write_right";
    let (h0, h1) = channel_create();

    // Dup h0 without WRITE
    let mut ro: HandleValue = 0;
    assert_ok(
        unsafe {
            zx_handle_duplicate(
                h0,
                ZX_RIGHT_READ | ZX_RIGHT_WAIT | ZX_RIGHT_INSPECT,
                &mut ro,
            )
        },
        name,
        b"dup",
    );

    let data = b"test";
    assert_status(
        unsafe {
            zx_channel_write(
                ro,
                0,
                data.as_ptr(),
                data.len() as u32,
                core::ptr::null(),
                0,
            )
        },
        ZX_ERR_ACCESS_DENIED,
        name,
        b"",
    );

    unsafe { zx_handle_close(ro) };
    unsafe { zx_handle_close(h0) };
    unsafe { zx_handle_close(h1) };
    pass(name);
}

/// Data exceeding 64KiB -> OUT_OF_RANGE
fn write_data_too_large() {
    let name = b"write_data_too_large";
    let (h0, h1) = channel_create();

    let big = vec![0u8; 65537];
    assert_status(
        unsafe { zx_channel_write(h0, 0, big.as_ptr(), big.len() as u32, core::ptr::null(), 0) },
        ZX_ERR_OUT_OF_RANGE,
        name,
        b"",
    );

    unsafe { zx_handle_close(h0) };
    unsafe { zx_handle_close(h1) };
    pass(name);
}

#[no_mangle]
pub fn main() {
    zx::debug_write(b"channel_extra_test: starting\n");

    write_and_read_via_syscall();
    write_handle_transfer();
    write_without_write_right();
    write_data_too_large();

    zx::debug_write(b"channel_extra_test: all tests passed\n");
}
