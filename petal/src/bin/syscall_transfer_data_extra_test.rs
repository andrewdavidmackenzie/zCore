//! petal transfer_data extra test -- rights enforcement and basic transfer.
//!
//! Port of zircon-tests/src/syscall_transfer_data_extra.rs.

#![no_std]
#![no_main]
#![allow(dead_code)]

extern crate alloc;
extern crate petal;

use alloc::vec;
use zx::sys::*;

fn pass(name: &[u8]) {
    zx::debug_write(b"  transfer_extra_test: ");
    zx::debug_write(name);
    zx::debug_write(b" PASS\n");
}

fn fail(name: &[u8], msg: &[u8]) {
    zx::debug_write(b"FAIL: transfer_extra_test: ");
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

fn vmo_create(size: u64) -> HandleValue {
    let mut h: HandleValue = 0;
    let s = unsafe { zx_vmo_create(size, 0, &mut h) };
    if s != ZX_OK {
        fail(b"vmo_create", b"failed");
    }
    h
}

/// Missing WRITE right on dst
fn missing_write_right_on_dst() {
    let name = b"missing_write_right_on_dst";
    let page_size = zx_system_get_page_size() as u64;
    let src = vmo_create(page_size);
    let dst = vmo_create(page_size);

    // Dup dst without WRITE
    let mut dst_ro: HandleValue = 0;
    assert_ok(
        unsafe {
            zx_handle_duplicate(
                dst,
                ZX_RIGHT_READ | ZX_RIGHT_WAIT | ZX_RIGHT_INSPECT,
                &mut dst_ro,
            )
        },
        name,
        b"dup dst",
    );

    assert_status(
        unsafe { zx_vmo_transfer_data(dst_ro, 0, 0, page_size, src, 0) },
        ZX_ERR_ACCESS_DENIED,
        name,
        b"",
    );

    unsafe { zx_handle_close(dst_ro) };
    unsafe { zx_handle_close(src) };
    unsafe { zx_handle_close(dst) };
    pass(name);
}

/// Missing READ right on src
fn missing_read_right_on_src() {
    let name = b"missing_read_right_on_src";
    let page_size = zx_system_get_page_size() as u64;
    let dst = vmo_create(page_size);
    let src = vmo_create(page_size);

    // Dup src without READ
    let mut src_wo: HandleValue = 0;
    assert_ok(
        unsafe {
            zx_handle_duplicate(
                src,
                ZX_RIGHT_WRITE | ZX_RIGHT_WAIT | ZX_RIGHT_INSPECT,
                &mut src_wo,
            )
        },
        name,
        b"dup src",
    );

    assert_status(
        unsafe { zx_vmo_transfer_data(dst, 0, 0, page_size, src_wo, 0) },
        ZX_ERR_ACCESS_DENIED,
        name,
        b"",
    );

    unsafe { zx_handle_close(src_wo) };
    unsafe { zx_handle_close(src) };
    unsafe { zx_handle_close(dst) };
    pass(name);
}

/// Wrong type (event instead of VMO)
fn wrong_type() {
    let name = b"wrong_type";
    let page_size = zx_system_get_page_size() as u64;
    let vmo = vmo_create(page_size);
    let mut event: HandleValue = 0;
    assert_ok(
        unsafe { zx_event_create(0, &mut event) },
        name,
        b"event create",
    );

    // Event as dst
    assert_status(
        unsafe { zx_vmo_transfer_data(event, 0, 0, page_size, vmo, 0) },
        ZX_ERR_WRONG_TYPE,
        name,
        b"event as dst",
    );

    // Event as src
    assert_status(
        unsafe { zx_vmo_transfer_data(vmo, 0, 0, page_size, event, 0) },
        ZX_ERR_WRONG_TYPE,
        name,
        b"event as src",
    );

    unsafe { zx_handle_close(event) };
    unsafe { zx_handle_close(vmo) };
    pass(name);
}

/// Basic transfer: write to src, transfer to dst, verify
fn basic_transfer() {
    let name = b"basic_transfer";
    let page_size = zx_system_get_page_size();
    let src = vmo_create(2 * page_size as u64);
    let dst = vmo_create(2 * page_size as u64);

    // Write data to src
    let src_data = vec![0xAAu8; page_size];
    assert_ok(
        unsafe { zx_vmo_write(src, src_data.as_ptr(), 0, page_size) },
        name,
        b"write src",
    );

    // Transfer first page of src to second page of dst
    assert_ok(
        unsafe { zx_vmo_transfer_data(dst, 0, page_size as u64, page_size as u64, src, 0) },
        name,
        b"transfer",
    );

    // Read dst second page — should have transferred data
    let mut read_buf = vec![0u8; page_size];
    assert_ok(
        unsafe { zx_vmo_read(dst, read_buf.as_mut_ptr(), page_size as u64, page_size) },
        name,
        b"read dst",
    );
    if read_buf != src_data {
        fail(name, b"dst data mismatch");
    }

    // Read src first page — should be zeroed
    let mut src_buf = vec![0xFFu8; page_size];
    assert_ok(
        unsafe { zx_vmo_read(src, src_buf.as_mut_ptr(), 0, page_size) },
        name,
        b"read src",
    );
    if src_buf.iter().any(|&b| b != 0) {
        fail(name, b"src not zeroed");
    }

    unsafe { zx_handle_close(src) };
    unsafe { zx_handle_close(dst) };
    pass(name);
}

#[no_mangle]
pub fn main() {
    zx::debug_write(b"transfer_extra_test: starting\n");

    missing_write_right_on_dst();
    missing_read_right_on_src();
    wrong_type();
    basic_transfer();

    zx::debug_write(b"transfer_extra_test: all tests passed\n");
}
