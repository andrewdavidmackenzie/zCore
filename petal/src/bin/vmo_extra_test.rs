//! petal VMO extra test -- VMO create_child syscall.
//!
//! Port of the UserPtr-using test from zircon-tests/src/vmo_extra.rs.

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

/// Create a VMO child (snapshot)
fn vmo_create_child_snapshot() {
    let name = b"vmo_create_child_snapshot";
    let page_size = zx_system_get_page_size() as u64;

    let mut parent: HandleValue = 0;
    assert_ok(
        unsafe { zx_vmo_create(page_size, 0, &mut parent) },
        name,
        b"create parent",
    );

    // Write data to parent
    let data = [0xABu8; 8];
    assert_ok(
        unsafe { zx_vmo_write(parent, data.as_ptr(), 0, 8) },
        name,
        b"write parent",
    );

    // Create snapshot child (option 1 = ZX_VMO_CHILD_SNAPSHOT)
    let mut child: HandleValue = 0;
    assert_ok(
        unsafe { zx_vmo_create_child(parent, 1, 0, page_size, &mut child) },
        name,
        b"create child",
    );

    // Read from child — should see parent's data
    let mut buf = [0u8; 8];
    assert_ok(
        unsafe { zx_vmo_read(child, buf.as_mut_ptr(), 0, 8) },
        name,
        b"read child",
    );
    if buf != data {
        fail(name, b"child data mismatch");
    }

    // Write to parent — child snapshot should be unaffected
    let new_data = [0xCDu8; 8];
    assert_ok(
        unsafe { zx_vmo_write(parent, new_data.as_ptr(), 0, 8) },
        name,
        b"write parent 2",
    );

    let mut buf2 = [0u8; 8];
    assert_ok(
        unsafe { zx_vmo_read(child, buf2.as_mut_ptr(), 0, 8) },
        name,
        b"read child 2",
    );
    if buf2 != data {
        fail(name, b"snapshot should be independent");
    }

    unsafe { zx_handle_close(child) };
    unsafe { zx_handle_close(parent) };
    pass(name);
}

/// VMO write and read
fn vmo_write_read() {
    let name = b"vmo_write_read";
    let page_size = zx_system_get_page_size() as u64;
    let mut h: HandleValue = 0;
    assert_ok(
        unsafe { zx_vmo_create(page_size, 0, &mut h) },
        name,
        b"create",
    );

    let data = b"hello vmo";
    assert_ok(
        unsafe { zx_vmo_write(h, data.as_ptr(), 0, data.len()) },
        name,
        b"write",
    );

    let mut buf = [0u8; 16];
    assert_ok(
        unsafe { zx_vmo_read(h, buf.as_mut_ptr(), 0, data.len()) },
        name,
        b"read",
    );
    if &buf[..data.len()] != data {
        fail(name, b"data mismatch");
    }

    unsafe { zx_handle_close(h) };
    pass(name);
}

/// VMO get/set size
fn vmo_get_set_size() {
    let name = b"vmo_get_set_size";
    let page_size = zx_system_get_page_size() as u64;
    let mut h: HandleValue = 0;
    // Create resizable VMO (option 0 is non-resizable, need bit for resizable)
    // Actually in zCore, VMOs are resizable by default unless explicitly non-resizable
    assert_ok(
        unsafe { zx_vmo_create(page_size, 0, &mut h) },
        name,
        b"create",
    );

    let mut size: usize = 0;
    assert_ok(unsafe { zx_vmo_get_size(h, &mut size) }, name, b"get size");
    if size as u64 != page_size {
        fail(name, b"initial size wrong");
    }

    unsafe { zx_handle_close(h) };
    pass(name);
}

#[no_mangle]
pub fn main() {
    zx::debug_write(b"vmo_extra_test: starting\n");
    vmo_create_child_snapshot();
    vmo_write_read();
    vmo_get_set_size();
    zx::debug_write(b"vmo_extra_test: all tests passed\n");
}
