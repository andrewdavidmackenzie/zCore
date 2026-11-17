//! petal object info extra test -- get_info with various topics.
//!
//! Port of UserPtr-using tests from zircon-tests/src/object_info_extra.rs.

#![no_std]
#![no_main]
#![allow(dead_code)]

extern crate petal;

use zx::sys::*;

const ZX_INFO_HANDLE_BASIC: u32 = 2;
const ZX_INFO_HANDLE_COUNT: u32 = 20;
const ZX_INFO_PROCESS: u32 = 3;

fn pass(name: &[u8]) {
    zx::debug_write(b"  obj_info_extra_test: ");
    zx::debug_write(name);
    zx::debug_write(b" PASS\n");
}

fn fail(name: &[u8], msg: &[u8]) {
    zx::debug_write(b"FAIL: obj_info_extra_test: ");
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

/// Handle basic on VMO
fn handle_basic_vmo() {
    let name = b"handle_basic_vmo";
    let mut h: HandleValue = 0;
    assert_ok(unsafe { zx_vmo_create(4096, 0, &mut h) }, name, b"create");

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
    if obj_type != 3 {
        fail(name, b"wrong type (expected 3=VMO)");
    }

    unsafe { zx_handle_close(h) };
    pass(name);
}

/// Handle basic on socket
fn handle_basic_socket() {
    let name = b"handle_basic_socket";
    let mut h0: HandleValue = 0;
    let mut h1: HandleValue = 0;
    assert_ok(
        unsafe { zx_socket_create(0, &mut h0, &mut h1) },
        name,
        b"create",
    );

    let mut buf = [0u8; 32];
    let mut actual: usize = 0;
    let mut avail: usize = 0;
    assert_ok(
        unsafe {
            zx_object_get_info(
                h0,
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
    if obj_type != 14 {
        fail(name, b"wrong type (expected 14=Socket)");
    }

    unsafe { zx_handle_close(h0) };
    unsafe { zx_handle_close(h1) };
    pass(name);
}

/// Handle basic on timer
fn handle_basic_timer() {
    let name = b"handle_basic_timer";
    let mut h: HandleValue = 0;
    assert_ok(unsafe { zx_timer_create(0, 0, &mut h) }, name, b"create");

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
    if obj_type != 22 {
        fail(name, b"wrong type (expected 22=Timer)");
    }

    unsafe { zx_handle_close(h) };
    pass(name);
}

/// Handle count
fn handle_count() {
    let name = b"handle_count";
    let mut h: HandleValue = 0;
    assert_ok(unsafe { zx_event_create(0, &mut h) }, name, b"create");

    let mut count: u32 = 0;
    let mut actual: usize = 0;
    let mut avail: usize = 0;
    assert_ok(
        unsafe {
            zx_object_get_info(
                h,
                ZX_INFO_HANDLE_COUNT,
                &mut count as *mut _ as *mut u8,
                4,
                &mut actual,
                &mut avail,
            )
        },
        name,
        b"get_info",
    );
    if count != 1 {
        fail(name, b"expected count 1");
    }

    // Duplicate and check count=2
    let mut dup: HandleValue = 0;
    assert_ok(
        unsafe { zx_handle_duplicate(h, ZX_RIGHT_SAME_RIGHTS, &mut dup) },
        name,
        b"dup",
    );

    assert_ok(
        unsafe {
            zx_object_get_info(
                h,
                ZX_INFO_HANDLE_COUNT,
                &mut count as *mut _ as *mut u8,
                4,
                &mut actual,
                &mut avail,
            )
        },
        name,
        b"get_info 2",
    );
    if count != 2 {
        fail(name, b"expected count 2 after dup");
    }

    unsafe { zx_handle_close(dup) };
    unsafe { zx_handle_close(h) };
    pass(name);
}

/// Invalid topic
fn invalid_topic() {
    let name = b"invalid_topic";
    let mut h: HandleValue = 0;
    assert_ok(unsafe { zx_event_create(0, &mut h) }, name, b"create");

    let mut buf = [0u8; 32];
    let mut actual: usize = 0;
    let mut avail: usize = 0;
    let s = unsafe { zx_object_get_info(h, 999, buf.as_mut_ptr(), 32, &mut actual, &mut avail) };
    if s == ZX_OK {
        fail(name, b"invalid topic should fail");
    }

    unsafe { zx_handle_close(h) };
    pass(name);
}

#[no_mangle]
pub fn main() {
    zx::debug_write(b"obj_info_extra_test: starting\n");

    handle_basic_vmo();
    handle_basic_socket();
    handle_basic_timer();
    handle_count();
    invalid_topic();

    zx::debug_write(b"obj_info_extra_test: all tests passed\n");
}
