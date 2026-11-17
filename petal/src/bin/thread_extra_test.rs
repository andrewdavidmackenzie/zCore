//! petal thread extra test -- thread create/info via syscall.
//!
//! Port of UserPtr-using tests from zircon-tests/src/thread_extra.rs.

#![no_std]
#![no_main]
#![allow(dead_code)]

extern crate petal;

use zx::sys::*;

const PA_PROC_SELF: u32 = 0x01;
const ZX_INFO_HANDLE_BASIC: u32 = 2;

fn pass(name: &[u8]) {
    zx::debug_write(b"  thread_extra_test: ");
    zx::debug_write(name);
    zx::debug_write(b" PASS\n");
}

fn fail(name: &[u8], msg: &[u8]) {
    zx::debug_write(b"FAIL: thread_extra_test: ");
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

/// Create thread and check handle type
fn create_thread_check_type(proc_h: HandleValue) {
    let name = b"create_thread_check_type";
    let tname = b"test-thread\0";
    let mut h: HandleValue = 0;
    assert_ok(
        unsafe { zx_thread_create(proc_h, tname.as_ptr(), 11, 0, &mut h) },
        name,
        b"create",
    );

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
    let obj_type = u32::from_le_bytes(buf[12..16].try_into().unwrap());
    if obj_type != 2 {
        fail(name, b"wrong type");
    }

    unsafe { zx_handle_close(h) };
    pass(name);
}

/// Thread create with invalid options
fn create_invalid_options(proc_h: HandleValue) {
    let name = b"create_invalid_options";
    let tname = b"test\0";
    let mut h: HandleValue = 0;
    let s = unsafe { zx_thread_create(proc_h, tname.as_ptr(), 4, 0xFF, &mut h) };
    if s == ZX_OK {
        unsafe { zx_handle_close(h) };
        fail(name, b"invalid options should fail");
    }
    pass(name);
}

/// Duplicate thread handle
fn duplicate_thread(proc_h: HandleValue) {
    let name = b"duplicate_thread";
    let tname = b"dup-t\0";
    let mut h: HandleValue = 0;
    assert_ok(
        unsafe { zx_thread_create(proc_h, tname.as_ptr(), 5, 0, &mut h) },
        name,
        b"create",
    );

    let mut dup: HandleValue = 0;
    assert_ok(
        unsafe { zx_handle_duplicate(h, ZX_RIGHT_SAME_RIGHTS, &mut dup) },
        name,
        b"dup",
    );
    if dup == 0 {
        fail(name, b"dup is 0");
    }

    unsafe { zx_handle_close(h) };
    unsafe { zx_handle_close(dup) };
    pass(name);
}

#[no_mangle]
pub fn main() {
    zx::debug_write(b"thread_extra_test: starting\n");

    let bootstrap = petal::Bootstrap::read().unwrap_or_else(|| {
        zx::debug_write(b"FAIL: no bootstrap\n");
        zx::Process::exit(1);
    });

    let proc_h = bootstrap.find(PA_PROC_SELF);
    if proc_h == 0 {
        zx::debug_write(b"  SKIP: no PA_PROC_SELF\n");
    } else {
        create_thread_check_type(proc_h);
        create_invalid_options(proc_h);
        duplicate_thread(proc_h);
    }

    zx::debug_write(b"thread_extra_test: all tests passed\n");
}
