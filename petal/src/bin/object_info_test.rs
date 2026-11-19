//! petal object_info test -- zx_object_get_info from user mode.
//!
//! Port of key UserPtr tests from zircon-tests/src/core_tests/object_info.rs.

#![no_std]
#![no_main]
#![allow(dead_code)]

extern crate petal;

use zx::sys::*;

const ZX_INFO_HANDLE_BASIC: u32 = 2;
const ZX_INFO_HANDLE_COUNT: u32 = 19;

fn pass(name: &[u8]) {
    zx::debug_write(b"  object_info_test: ");
    zx::debug_write(name);
    zx::debug_write(b" PASS\n");
}

fn fail(name: &[u8], msg: &[u8]) {
    zx::debug_write(b"FAIL: object_info_test: ");
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

/// Get handle basic info on an event
fn get_info_handle_basic_event() {
    let name = b"get_info_handle_basic_event";
    let mut h: HandleValue = 0;
    assert_ok(unsafe { zx_event_create(0, &mut h) }, name, b"create");

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
    if actual != 1 || avail != 1 {
        fail(name, b"wrong actual/avail");
    }

    // obj_type at offset 12: Event = 5
    let obj_type = u32::from_le_bytes(buf[12..16].try_into().unwrap());
    if obj_type != 5 {
        fail(name, b"wrong obj_type");
    }

    // koid at offset 0 should be non-zero
    let koid = u64::from_le_bytes(buf[0..8].try_into().unwrap());
    if koid == 0 {
        fail(name, b"koid is 0");
    }

    unsafe { zx_handle_close(h) };
    pass(name);
}

/// Get info on bad handle
fn get_info_bad_handle() {
    let name = b"get_info_bad_handle";
    let mut buf = [0u8; 32];
    let mut actual: usize = 0;
    let mut avail: usize = 0;
    assert_status(
        unsafe {
            zx_object_get_info(
                0xDEAD,
                ZX_INFO_HANDLE_BASIC,
                buf.as_mut_ptr(),
                32,
                &mut actual,
                &mut avail,
            )
        },
        ZX_ERR_BAD_HANDLE,
        name,
        b"",
    );
    pass(name);
}

/// Get info with null buffer (zero size) -> BUFFER_TOO_SMALL
fn get_info_zero_buffer() {
    let name = b"get_info_zero_buffer";
    let mut h: HandleValue = 0;
    assert_ok(unsafe { zx_event_create(0, &mut h) }, name, b"create");

    let mut actual: usize = 0;
    let mut avail: usize = 0;
    assert_status(
        unsafe {
            zx_object_get_info(
                h,
                ZX_INFO_HANDLE_BASIC,
                core::ptr::null_mut(),
                0,
                &mut actual,
                &mut avail,
            )
        },
        ZX_ERR_BUFFER_TOO_SMALL,
        name,
        b"",
    );
    // avail should be 1
    if avail != 1 {
        fail(name, b"avail should be 1");
    }

    unsafe { zx_handle_close(h) };
    pass(name);
}

/// Get info with null actual/avail pointers
fn get_info_null_actual_avail() {
    let name = b"get_info_null_actual_avail";
    let mut h: HandleValue = 0;
    assert_ok(unsafe { zx_event_create(0, &mut h) }, name, b"create");

    let mut buf = [0u8; 32];
    // Null actual and avail should be accepted
    assert_ok(
        unsafe {
            zx_object_get_info(
                h,
                ZX_INFO_HANDLE_BASIC,
                buf.as_mut_ptr(),
                32,
                core::ptr::null_mut(),
                core::ptr::null_mut(),
            )
        },
        name,
        b"",
    );

    unsafe { zx_handle_close(h) };
    pass(name);
}

/// Get info with kernel address buffer -> should fail
fn get_info_kernel_addr_buffer() {
    let name = b"get_info_kernel_addr_buffer";
    let mut h: HandleValue = 0;
    assert_ok(unsafe { zx_event_create(0, &mut h) }, name, b"create");

    let mut actual: usize = 0;
    let mut avail: usize = 0;
    let kernel_addr = 0xffff_8000_0000_0000_usize as *mut u8;
    let s = unsafe {
        zx_object_get_info(
            h,
            ZX_INFO_HANDLE_BASIC,
            kernel_addr,
            32,
            &mut actual,
            &mut avail,
        )
    };
    if s == ZX_OK {
        fail(name, b"kernel addr should fail");
    }

    unsafe { zx_handle_close(h) };
    pass(name);
}

/// Get handle count info
fn get_info_handle_count() {
    let name = b"get_info_handle_count";
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
    if count == 0 {
        fail(name, b"count is 0");
    }

    unsafe { zx_handle_close(h) };
    pass(name);
}

/// Get info on various object types
fn get_info_channel_type() {
    let name = b"get_info_channel_type";
    let mut h0: HandleValue = 0;
    let mut h1: HandleValue = 0;
    assert_ok(
        unsafe { zx_channel_create(0, &mut h0, &mut h1) },
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
    if obj_type != 4 {
        fail(name, b"wrong type (expected 4=Channel)");
    }

    // Check related koid
    let related = u64::from_le_bytes(buf[16..24].try_into().unwrap());
    if related == 0 {
        fail(name, b"related koid is 0");
    }

    unsafe { zx_handle_close(h0) };
    unsafe { zx_handle_close(h1) };
    pass(name);
}

#[no_mangle]
pub fn main() {
    zx::debug_write(b"object_info_test: starting\n");

    get_info_handle_basic_event();
    get_info_bad_handle();
    get_info_zero_buffer();
    get_info_null_actual_avail();
    get_info_kernel_addr_buffer();
    get_info_handle_count();
    get_info_channel_type();

    zx::debug_write(b"object_info_test: all tests passed\n");
}
