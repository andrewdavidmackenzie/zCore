//! petal VMO syscall test -- exercises VMO stream size from user mode.
//!
//! Port of zircon-tests/src/core_tests/syscall_vmo.rs.

#![no_std]
#![no_main]
#![allow(dead_code)]

extern crate petal;

use zx::sys::*;

fn pass(name: &[u8]) {
    zx::debug_write(b"  syscall_vmo_test: ");
    zx::debug_write(name);
    zx::debug_write(b" PASS\n");
}

fn fail(name: &[u8], msg: &[u8]) {
    zx::debug_write(b"FAIL: syscall_vmo_test: ");
    zx::debug_write(name);
    zx::debug_write(b" - ");
    zx::debug_write(msg);
    zx::debug_write(b"\n");
    zx::Process::exit(1);
}

fn assert_ok(status: ZxStatus, name: &[u8], msg: &[u8]) {
    if status != ZX_OK {
        fail(name, msg);
    }
}

/// C++: TEST(VmoTestCase, StreamSize) -- syscall level
fn stream_size_via_syscall() {
    let name = b"stream_size_via_syscall";
    let page_size = zx_system_get_page_size();

    // Create a 4-page VMO
    let mut h: HandleValue = 0;
    assert_ok(
        unsafe { zx_vmo_create((page_size * 4) as u64, 0, &mut h) },
        name,
        b"vmo create",
    );

    // Set stream size to 2 pages
    assert_ok(
        unsafe { zx_vmo_set_stream_size(h, (page_size * 2) as u64) },
        name,
        b"set stream size",
    );

    // Get stream size -- should be 2 pages
    let mut size: u64 = 0;
    assert_ok(
        unsafe { zx_vmo_get_stream_size(h, &mut size) },
        name,
        b"get stream size",
    );
    if size != (page_size * 2) as u64 {
        fail(name, b"stream size mismatch");
    }

    unsafe { zx_handle_close(h) };
    pass(name);
}

#[no_mangle]
pub fn main() {
    zx::debug_write(b"syscall_vmo_test: starting\n");
    stream_size_via_syscall();
    zx::debug_write(b"syscall_vmo_test: all tests passed\n");
}
