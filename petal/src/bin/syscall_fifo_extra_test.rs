//! petal FIFO extra test -- write/read, elem_size mismatch, peer closed.
//!
//! Port of zircon-tests/src/syscall_fifo_extra.rs.

#![no_std]
#![no_main]
#![allow(dead_code)]

extern crate petal;

use zx::sys::*;

fn pass(name: &[u8]) {
    zx::debug_write(b"  fifo_extra_test: ");
    zx::debug_write(name);
    zx::debug_write(b" PASS\n");
}

fn fail(name: &[u8], msg: &[u8]) {
    zx::debug_write(b"FAIL: fifo_extra_test: ");
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

fn fifo_create(count: usize, elem_size: usize) -> (HandleValue, HandleValue) {
    let mut h0: HandleValue = 0;
    let mut h1: HandleValue = 0;
    let s = unsafe { zx_fifo_create(count, elem_size, 0, &mut h0, &mut h1) };
    if s != ZX_OK {
        fail(b"fifo_create", b"failed");
    }
    (h0, h1)
}

/// Write 3 elements, read them back
fn write_and_read() {
    let name = b"write_and_read";
    let (h0, h1) = fifo_create(16, 4);

    let data: [u32; 3] = [1, 2, 3];
    let mut actual: usize = 0;
    assert_ok(
        unsafe { zx_fifo_write(h0, 4, data.as_ptr() as *const u8, 3, &mut actual) },
        name,
        b"write",
    );
    if actual != 3 {
        fail(name, b"write actual != 3");
    }

    let mut buf = [0u32; 3];
    assert_ok(
        unsafe { zx_fifo_read(h1, 4, buf.as_mut_ptr() as *mut u8, 3, &mut actual) },
        name,
        b"read",
    );
    if actual != 3 {
        fail(name, b"read actual != 3");
    }
    if buf != [1, 2, 3] {
        fail(name, b"data mismatch");
    }

    unsafe { zx_handle_close(h0) };
    unsafe { zx_handle_close(h1) };
    pass(name);
}

/// Wrong elem_size -> OUT_OF_RANGE
fn elem_size_mismatch() {
    let name = b"elem_size_mismatch";
    let (h0, h1) = fifo_create(16, 4);

    let data = [0u8; 8];
    let mut actual: usize = 0;
    let s = unsafe { zx_fifo_write(h0, 8, data.as_ptr(), 1, &mut actual) };
    if s != ZX_ERR_OUT_OF_RANGE {
        fail(name, b"expected OUT_OF_RANGE");
    }

    unsafe { zx_handle_close(h0) };
    unsafe { zx_handle_close(h1) };
    pass(name);
}

/// Write to peer-closed FIFO -> PEER_CLOSED
fn peer_closed() {
    let name = b"peer_closed";
    let (h0, h1) = fifo_create(16, 4);
    unsafe { zx_handle_close(h1) };

    let data = [0u8; 4];
    let mut actual: usize = 0;
    let s = unsafe { zx_fifo_write(h0, 4, data.as_ptr(), 1, &mut actual) };
    if s != ZX_ERR_PEER_CLOSED {
        fail(name, b"expected PEER_CLOSED");
    }

    unsafe { zx_handle_close(h0) };
    pass(name);
}

#[no_mangle]
pub fn main() {
    zx::debug_write(b"fifo_extra_test: starting\n");
    write_and_read();
    elem_size_mismatch();
    peer_closed();
    zx::debug_write(b"fifo_extra_test: all tests passed\n");
}
