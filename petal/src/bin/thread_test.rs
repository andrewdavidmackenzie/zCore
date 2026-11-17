//! petal thread test -- thread create, name, info.
//!
//! Port of sync tests from zircon-tests/src/core_tests/thread.rs.

#![no_std]
#![no_main]
#![allow(dead_code)]

extern crate petal;

use zx::sys::*;

const PA_PROC_SELF: u32 = 0x01;
const ZX_INFO_HANDLE_BASIC: u32 = 2;
const ZX_PROP_NAME: u32 = 3;

fn pass(name: &[u8]) {
    zx::debug_write(b"  thread_test: ");
    zx::debug_write(name);
    zx::debug_write(b" PASS\n");
}

fn fail(name: &[u8], msg: &[u8]) {
    zx::debug_write(b"FAIL: thread_test: ");
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

/// Create thread with empty name
fn empty_name_succeeds(proc_h: HandleValue) {
    let name = b"empty_name_succeeds";
    let tname = b"\0";
    let mut h: HandleValue = 0;
    assert_ok(
        unsafe { zx_thread_create(proc_h, tname.as_ptr(), 0, 0, &mut h) },
        name,
        b"create",
    );
    if h == 0 {
        fail(name, b"handle is 0");
    }
    unsafe { zx_handle_close(h) };
    pass(name);
}

/// Create thread with long name
fn long_name_succeeds(proc_h: HandleValue) {
    let name = b"long_name_succeeds";
    let long = b"this-is-a-very-long-thread-name-that-exceeds-the-limit\0";
    let mut h: HandleValue = 0;
    assert_ok(
        unsafe { zx_thread_create(proc_h, long.as_ptr(), long.len() - 1, 0, &mut h) },
        name,
        b"create",
    );

    // Get name back
    let mut buf = [0u8; 64];
    assert_ok(
        unsafe { zx_object_get_property(h, ZX_PROP_NAME, buf.as_mut_ptr(), 64) },
        name,
        b"get name",
    );
    if &buf[..4] != b"this" {
        fail(name, b"name prefix wrong");
    }

    unsafe { zx_handle_close(h) };
    pass(name);
}

/// Not-started thread info
fn nonstarted_thread(proc_h: HandleValue) {
    let name = b"nonstarted_thread";
    let tname = b"unstarted\0";
    let mut h: HandleValue = 0;
    assert_ok(
        unsafe { zx_thread_create(proc_h, tname.as_ptr(), 9, 0, &mut h) },
        name,
        b"create",
    );

    // Get basic info
    let mut buf = [0u8; 32];
    let mut actual: usize = 0;
    let mut avail: usize = 0;
    assert_ok(
        unsafe {
            zx_object_get_info(
                h,
                ZX_INFO_HANDLE_BASIC,
                buf.as_mut_ptr(),
                32,
                &mut actual,
                &mut avail,
            )
        },
        name,
        b"get_info",
    );
    // obj_type at offset 12: Thread = 2
    let obj_type = u32::from_le_bytes(buf[12..16].try_into().unwrap());
    if obj_type != 2 {
        fail(name, b"wrong obj_type (expected 2=Thread)");
    }

    // koid should be non-zero
    let koid = u64::from_le_bytes(buf[0..8].try_into().unwrap());
    if koid == 0 {
        fail(name, b"koid is 0");
    }

    unsafe { zx_handle_close(h) };
    pass(name);
}

/// Bad handle for thread create
fn create_bad_handle() {
    let name = b"create_bad_handle";
    let tname = b"test\0";
    let mut h: HandleValue = 0;
    let s = unsafe { zx_thread_create(0xDEAD, tname.as_ptr(), 4, 0, &mut h) };
    if s != ZX_ERR_BAD_HANDLE {
        fail(name, b"expected BAD_HANDLE");
    }
    pass(name);
}

/// Wrong type for thread create (event instead of process)
fn create_wrong_type() {
    let name = b"create_wrong_type";
    let mut event: HandleValue = 0;
    assert_ok(
        unsafe { zx_event_create(0, &mut event) },
        name,
        b"event create",
    );
    let tname = b"test\0";
    let mut h: HandleValue = 0;
    let s = unsafe { zx_thread_create(event, tname.as_ptr(), 4, 0, &mut h) };
    if s != ZX_ERR_WRONG_TYPE {
        fail(name, b"expected WRONG_TYPE");
    }
    unsafe { zx_handle_close(event) };
    pass(name);
}

#[no_mangle]
pub fn main() {
    zx::debug_write(b"thread_test: starting\n");

    let bootstrap = petal::Bootstrap::read().unwrap_or_else(|| {
        zx::debug_write(b"FAIL: no bootstrap\n");
        zx::Process::exit(1);
    });

    let proc_h = bootstrap.find(PA_PROC_SELF);
    if proc_h == 0 {
        zx::debug_write(b"  SKIP: no PA_PROC_SELF\n");
    } else {
        empty_name_succeeds(proc_h);
        long_name_succeeds(proc_h);
        nonstarted_thread(proc_h);
    }

    create_bad_handle();
    create_wrong_type();

    zx::debug_write(b"thread_test: all tests passed\n");
}
