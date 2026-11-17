//! petal VMO extra test -- op_range commit/decommit/bounds.
//!
//! Port of sync tests from zircon-tests/src/syscall_vmo_extra.rs.

#![no_std]
#![no_main]
#![allow(dead_code)]

extern crate petal;

use zx::sys::*;

fn pass(name: &[u8]) {
    zx::debug_write(b"  vmo_extra_test: ");
    zx::debug_write(name);
    zx::debug_write(b" PASS\n");
}

fn fail(name: &[u8], msg: &[u8]) {
    zx::debug_write(b"FAIL: vmo_extra_test: ");
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

fn vmo_create(size: u64) -> HandleValue {
    let mut h: HandleValue = 0;
    let s = unsafe { zx_vmo_create(size, 0, &mut h) };
    if s != ZX_OK {
        fail(b"vmo_create", b"failed");
    }
    h
}

/// ZX_VMO_OP_COMMIT/DECOMMIT
fn op_range_commit_decommit() {
    let name = b"op_range_commit_decommit";
    let page_size = zx_system_get_page_size() as u64;
    let h = vmo_create(4 * page_size);

    // Commit first page (op=1 = ZX_VMO_OP_COMMIT)
    assert_ok(
        unsafe { zx_vmo_op_range(h, 1, 0, page_size, core::ptr::null_mut(), 0) },
        name,
        b"commit",
    );

    // Decommit first page (op=2 = ZX_VMO_OP_DECOMMIT)
    assert_ok(
        unsafe { zx_vmo_op_range(h, 2, 0, page_size, core::ptr::null_mut(), 0) },
        name,
        b"decommit",
    );

    unsafe { zx_handle_close(h) };
    pass(name);
}

/// op_range out of bounds
fn op_range_out_of_bounds() {
    let name = b"op_range_out_of_bounds";
    let page_size = zx_system_get_page_size() as u64;
    let h = vmo_create(page_size);

    // Commit beyond VMO size -> OUT_OF_RANGE
    let s = unsafe { zx_vmo_op_range(h, 1, page_size, page_size, core::ptr::null_mut(), 0) };
    if s == ZX_OK {
        fail(name, b"commit beyond size should fail");
    }

    unsafe { zx_handle_close(h) };
    pass(name);
}

/// VMO read without READ right
fn read_without_read_right() {
    let name = b"read_without_read_right";
    let h = vmo_create(4096);

    // Dup without READ
    let mut wo: HandleValue = 0;
    assert_ok(
        unsafe {
            zx_handle_duplicate(
                h,
                ZX_RIGHT_WRITE | ZX_RIGHT_WAIT | ZX_RIGHT_INSPECT,
                &mut wo,
            )
        },
        name,
        b"dup",
    );

    let mut buf = [0u8; 16];
    let s = unsafe { zx_vmo_read(wo, buf.as_mut_ptr(), 0, buf.len()) };
    if s != ZX_ERR_ACCESS_DENIED {
        fail(name, b"expected ACCESS_DENIED");
    }

    unsafe { zx_handle_close(wo) };
    unsafe { zx_handle_close(h) };
    pass(name);
}

/// VMO write without WRITE right
fn write_without_write_right() {
    let name = b"write_without_write_right";
    let h = vmo_create(4096);

    // Dup without WRITE
    let mut ro: HandleValue = 0;
    assert_ok(
        unsafe {
            zx_handle_duplicate(h, ZX_RIGHT_READ | ZX_RIGHT_WAIT | ZX_RIGHT_INSPECT, &mut ro)
        },
        name,
        b"dup",
    );

    let data = [0xAAu8; 16];
    let s = unsafe { zx_vmo_write(ro, data.as_ptr(), 0, data.len()) };
    if s != ZX_ERR_ACCESS_DENIED {
        fail(name, b"expected ACCESS_DENIED");
    }

    unsafe { zx_handle_close(ro) };
    unsafe { zx_handle_close(h) };
    pass(name);
}

#[no_mangle]
pub fn main() {
    zx::debug_write(b"vmo_extra_test: starting\n");

    op_range_commit_decommit();
    op_range_out_of_bounds();
    read_without_read_right();
    write_without_write_right();

    zx::debug_write(b"vmo_extra_test: all tests passed\n");
}
