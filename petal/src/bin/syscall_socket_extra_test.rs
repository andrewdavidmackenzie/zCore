//! petal socket extra test -- shutdown write.
//!
//! Port of zircon-tests/src/syscall_socket_extra.rs.

#![no_std]
#![no_main]
#![allow(dead_code)]

extern crate petal;

use zx::sys::*;

fn pass(name: &[u8]) {
    zx::debug_write(b"  socket_extra_test: ");
    zx::debug_write(name);
    zx::debug_write(b" PASS\n");
}

fn fail(name: &[u8], msg: &[u8]) {
    zx::debug_write(b"FAIL: socket_extra_test: ");
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

/// Shutdown write on h0, verify h0 can't write but h1 still can
fn shutdown_write() {
    let name = b"shutdown_write";
    let mut h0: HandleValue = 0;
    let mut h1: HandleValue = 0;
    assert_ok(
        unsafe { zx_socket_create(0, &mut h0, &mut h1) },
        name,
        b"create",
    );

    // Shutdown write on h0 (flag 1 = SHUTDOWN_WRITE)
    assert_ok(unsafe { zx_socket_shutdown(h0, 1) }, name, b"shutdown");

    // h0 can no longer write
    let data = b"fail";
    let mut actual: usize = 0;
    let s = unsafe { zx_socket_write(h0, 0, data.as_ptr(), data.len(), &mut actual) };
    if s != ZX_ERR_BAD_STATE {
        fail(name, b"expected BAD_STATE after shutdown");
    }

    // h1 can still write to h0
    let data2 = b"ok";
    assert_ok(
        unsafe { zx_socket_write(h1, 0, data2.as_ptr(), data2.len(), &mut actual) },
        name,
        b"h1 write",
    );
    if actual != 2 {
        fail(name, b"h1 write actual != 2");
    }

    unsafe { zx_handle_close(h0) };
    unsafe { zx_handle_close(h1) };
    pass(name);
}

#[no_mangle]
pub fn main() {
    zx::debug_write(b"socket_extra_test: starting\n");
    shutdown_write();
    zx::debug_write(b"socket_extra_test: all tests passed\n");
}
