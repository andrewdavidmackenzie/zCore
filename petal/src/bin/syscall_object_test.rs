//! petal object syscall test -- get_info, properties, signal, close_many.
//!
//! Port of zircon-tests/src/syscall_object.rs.

#![no_std]
#![no_main]
#![allow(dead_code)]

extern crate petal;

use zx::sys::*;

const ZX_INFO_HANDLE_BASIC: u32 = 2;
const ZX_PROP_NAME: u32 = 3;

fn pass(name: &[u8]) {
    zx::debug_write(b"  syscall_object_test: ");
    zx::debug_write(name);
    zx::debug_write(b" PASS\n");
}

fn fail(name: &[u8], msg: &[u8]) {
    zx::debug_write(b"FAIL: syscall_object_test: ");
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

/// ZX_INFO_HANDLE_BASIC on channel
fn get_info_handle_basic() {
    let name = b"get_info_handle_basic";
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
    if actual != 1 || avail != 1 {
        fail(name, b"actual/avail wrong");
    }

    // obj_type at offset 12: Channel = 4
    let obj_type = u32::from_le_bytes(buf[12..16].try_into().unwrap());
    if obj_type != 4 {
        fail(name, b"wrong obj_type (expected 4=Channel)");
    }

    unsafe { zx_handle_close(h0) };
    unsafe { zx_handle_close(h1) };
    pass(name);
}

/// ZX_INFO_HANDLE_BASIC on invalid handle
fn get_info_bad_handle() {
    let name = b"get_info_bad_handle";
    let mut buf = [0u8; 32];
    let mut actual: usize = 0;
    let mut avail: usize = 0;
    assert_status(
        unsafe {
            zx_object_get_info(
                0xDEAD_BEEF,
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

/// ZX_PROP_NAME get/set
fn set_get_property_name() {
    let name = b"set_get_property_name";
    // ZX_PROP_NAME applies to jobs/processes/threads — channels do not
    // have the SET_PROPERTY right. Use the bootstrap process handle.
    let bootstrap = petal::Bootstrap::read().unwrap_or_else(|| {
        zx::debug_write(b"FAIL: no bootstrap\n");
        zx::Process::exit(1);
    });
    let proc_h = bootstrap.find(zircon_abi::processargs::PA_PROC_SELF);
    if proc_h == 0 {
        zx::debug_write(b"  SKIP: no PA_PROC_SELF\n");
        return;
    }

    // Set name
    let obj_name = b"test-proc\0";
    assert_ok(
        unsafe { zx_object_set_property(proc_h, ZX_PROP_NAME, obj_name.as_ptr(), obj_name.len()) },
        name,
        b"set name",
    );

    // Get name
    let mut buf = [0u8; 32];
    assert_ok(
        unsafe { zx_object_get_property(proc_h, ZX_PROP_NAME, buf.as_mut_ptr(), buf.len()) },
        name,
        b"get name",
    );
    if &buf[..9] != b"test-proc" {
        fail(name, b"name mismatch");
    }

    pass(name);
}

/// Signal self
fn object_signal_self() {
    let name = b"object_signal_self";
    let mut h0: HandleValue = 0;
    let mut h1: HandleValue = 0;
    assert_ok(
        unsafe { zx_channel_create(0, &mut h0, &mut h1) },
        name,
        b"create",
    );

    assert_ok(
        unsafe { zx_object_signal(h0, 0, 1 << 24) },
        name,
        b"set signal",
    );
    assert_ok(
        unsafe { zx_object_signal(h0, 1 << 24, 0) },
        name,
        b"clear signal",
    );

    unsafe { zx_handle_close(h0) };
    unsafe { zx_handle_close(h1) };
    pass(name);
}

/// Signal with non-user bits -> INVALID_ARGS
fn object_signal_invalid_bits() {
    let name = b"object_signal_invalid_bits";
    let mut h0: HandleValue = 0;
    let mut h1: HandleValue = 0;
    assert_ok(
        unsafe { zx_channel_create(0, &mut h0, &mut h1) },
        name,
        b"create",
    );

    // READABLE (bit 0) is kernel signal
    assert_status(
        unsafe { zx_object_signal(h0, 0, 1) },
        ZX_ERR_INVALID_ARGS,
        name,
        b"",
    );

    unsafe { zx_handle_close(h0) };
    unsafe { zx_handle_close(h1) };
    pass(name);
}

/// Close many handles at once
fn close_many() {
    let name = b"close_many";
    let mut h0: HandleValue = 0;
    let mut h1: HandleValue = 0;
    let mut h2: HandleValue = 0;
    let mut h3: HandleValue = 0;
    let mut h4: HandleValue = 0;
    let mut h5: HandleValue = 0;
    assert_ok(
        unsafe { zx_channel_create(0, &mut h0, &mut h1) },
        name,
        b"create 0",
    );
    assert_ok(
        unsafe { zx_channel_create(0, &mut h2, &mut h3) },
        name,
        b"create 1",
    );
    assert_ok(
        unsafe { zx_channel_create(0, &mut h4, &mut h5) },
        name,
        b"create 2",
    );

    let handles = [h0, h1, h2, h3, h4, h5];
    assert_ok(
        unsafe { zx_handle_close_many(handles.as_ptr(), handles.len()) },
        name,
        b"close_many",
    );

    // All should be gone
    for &h in &handles {
        let s = unsafe { zx_handle_check_valid(h) };
        if s == ZX_OK {
            fail(name, b"handle still valid");
        }
    }

    pass(name);
}

/// handle_check_valid
fn handle_check_valid_test() {
    let name = b"handle_check_valid";
    let mut h0: HandleValue = 0;
    let mut h1: HandleValue = 0;
    assert_ok(
        unsafe { zx_channel_create(0, &mut h0, &mut h1) },
        name,
        b"create",
    );

    // Valid handle
    assert_ok(unsafe { zx_handle_check_valid(h0) }, name, b"valid");

    // Handle 0 is INVALID
    let s = unsafe { zx_handle_check_valid(0) };
    if s == ZX_OK {
        fail(name, b"handle 0 should be invalid");
    }

    unsafe { zx_handle_close(h0) };
    unsafe { zx_handle_close(h1) };
    pass(name);
}

#[no_mangle]
pub fn main() {
    zx::debug_write(b"syscall_object_test: starting\n");

    get_info_handle_basic();
    get_info_bad_handle();
    set_get_property_name();
    object_signal_self();
    object_signal_invalid_bits();
    close_many();
    handle_check_valid_test();

    zx::debug_write(b"syscall_object_test: all tests passed\n");
}
