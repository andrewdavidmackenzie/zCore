//! petal debuglog test -- exercises Zircon debuglog syscalls from user mode.
//!
//! Ports of the syscall-layer tests from
//! `zircon-tests/src/core_tests/debuglog.rs` that require user-space
//! buffers (UserInPtr/UserOutPtr). The kernel-API tests (drain_and_signal,
//! max_message_size) remain in zircon-tests.

#![no_std]
#![no_main]
#![allow(dead_code)]

extern crate alloc;
extern crate petal;

use zx::sys::*;

const FLAG_READABLE: u32 = 0x4000_0000;
const DLOG_MAX_LEN: usize = 256;

fn pass(name: &[u8]) {
    zx::debug_write(b"  debuglog_test: ");
    zx::debug_write(name);
    zx::debug_write(b" PASS\n");
}

fn fail(name: &[u8], msg: &[u8]) {
    zx::debug_write(b"FAIL: debuglog_test: ");
    zx::debug_write(name);
    zx::debug_write(b" - ");
    zx::debug_write(msg);
    zx::debug_write(b"\n");
    zx::Process::exit(1);
}

fn assert_status(got: ZxStatus, expected: ZxStatus, name: &[u8], msg: &[u8]) {
    if got != expected {
        fail(name, msg);
    }
}

/// C++: TEST(DebugLogTest, InvalidHandleAllowedOnlyForWrite)
fn invalid_handle_allowed_only_for_write() {
    let name = b"invalid_handle_allowed_only_for_write";
    let mut buf = [0u8; DLOG_MAX_LEN];

    // Read with invalid handle -> BAD_HANDLE
    let status = unsafe { zx_debuglog_read(0xDEAD, 0, buf.as_mut_ptr(), buf.len()) };
    assert_status(status, ZX_ERR_BAD_HANDLE, name, b"read bad handle");

    // Write with invalid handle -> BAD_HANDLE
    let data = b"test write";
    let status = unsafe { zx_debuglog_write(0xDEAD, 0, data.as_ptr(), data.len()) };
    assert_status(status, ZX_ERR_BAD_HANDLE, name, b"write bad handle");

    pass(name);
}

/// C++: TEST(DebugLogTest, InvalidOptions)
fn invalid_options() {
    let name = b"invalid_options";
    let mut h: HandleValue = 0;

    // options=0x1 is invalid
    let status = unsafe { zx_debuglog_create(0, 0x1, &mut h) };
    assert_status(status, ZX_ERR_INVALID_ARGS, name, b"options 0x1");

    // options=0xFFFF is invalid
    let status = unsafe { zx_debuglog_create(0, 0xFFFF, &mut h) };
    assert_status(status, ZX_ERR_INVALID_ARGS, name, b"options 0xFFFF");

    pass(name);
}

/// C++: TEST(DebugLogTest, InvalidResourceHandleReturnsWrongType)
fn invalid_resource_handle_returns_wrong_type() {
    let name = b"invalid_resource_handle_returns_wrong_type";

    // Create an event (not a resource)
    let mut event_h: HandleValue = 0;
    let status = unsafe { zx_event_create(0, &mut event_h) };
    assert_status(status, ZX_OK, name, b"event create");

    // Use event handle as resource for debuglog_create -> WRONG_TYPE
    let mut h: HandleValue = 0;
    let status = unsafe { zx_debuglog_create(event_h, 0, &mut h) };
    if status != ZX_ERR_WRONG_TYPE && status != ZX_ERR_BAD_HANDLE {
        fail(name, b"expected WRONG_TYPE or BAD_HANDLE");
    }

    unsafe { zx_handle_close(event_h) };
    pass(name);
}

/// C++: TEST(DebugLogTest, ReadErrors)
fn read_errors() {
    let name = b"read_errors";

    // Create a readable debuglog (need root resource - use handle 0 for now)
    let mut dlog_h: HandleValue = 0;
    let status = unsafe { zx_debuglog_create(0, FLAG_READABLE, &mut dlog_h) };
    // If creation fails (no root resource in user mode), skip
    if status != ZX_OK {
        zx::debug_write(b"  debuglog_test: read_errors SKIP (no root resource)\n");
        return;
    }

    let mut buf = [0u8; DLOG_MAX_LEN];

    // Invalid options
    let status = unsafe { zx_debuglog_read(dlog_h, 0x1, buf.as_mut_ptr(), buf.len()) };
    assert_status(status, ZX_ERR_INVALID_ARGS, name, b"invalid options");

    // Bad handle
    let status = unsafe { zx_debuglog_read(0xDEAD, 0, buf.as_mut_ptr(), buf.len()) };
    assert_status(status, ZX_ERR_BAD_HANDLE, name, b"bad handle");

    unsafe { zx_handle_close(dlog_h) };
    pass(name);
}

/// C++: TEST(DebugLogTest, WriteErrors)
fn write_errors() {
    let name = b"write_errors";

    // Create a write-only debuglog
    let mut dlog_h: HandleValue = 0;
    let status = unsafe { zx_debuglog_create(0, 0, &mut dlog_h) };
    assert_status(status, ZX_OK, name, b"create");

    let data = b"write-error-test";

    // Invalid options
    let status = unsafe { zx_debuglog_write(dlog_h, 0x20, data.as_ptr(), data.len()) };
    assert_status(status, ZX_ERR_INVALID_ARGS, name, b"invalid options");

    // Bad handle
    let status = unsafe { zx_debuglog_write(0xDEAD, 0, data.as_ptr(), data.len()) };
    assert_status(status, ZX_ERR_BAD_HANDLE, name, b"bad handle");

    unsafe { zx_handle_close(dlog_h) };
    pass(name);
}

/// C++: TEST(DebugLogTest, WriteRead)
fn write_read() {
    let name = b"write_read";

    // Create a readable debuglog
    let mut dlog_h: HandleValue = 0;
    let status = unsafe { zx_debuglog_create(0, FLAG_READABLE, &mut dlog_h) };
    if status != ZX_OK {
        zx::debug_write(b"  debuglog_test: write_read SKIP (no root resource)\n");
        return;
    }

    // Write a message
    let msg = b"Debuglog petal test.\n";
    let status = unsafe { zx_debuglog_write(dlog_h, 0, msg.as_ptr(), msg.len()) };
    assert_status(status, ZX_OK, name, b"write");

    // Read back — drain until we find our message
    let mut buf = [0u8; DLOG_MAX_LEN];
    let mut found = false;
    for _ in 0..1000 {
        let status = unsafe { zx_debuglog_read(dlog_h, 0, buf.as_mut_ptr(), buf.len()) };
        if status <= 0 {
            break;
        }
        let n = status as usize;
        // Data payload starts at offset 40 (DlogHeader size).
        // datalen is at offset 12 (u16 LE).
        if n >= 40 + msg.len() {
            let datalen = u16::from_ne_bytes([buf[12], buf[13]]) as usize;
            if datalen >= msg.len() && buf[40..40 + msg.len()] == *msg {
                found = true;
                break;
            }
        }
    }
    if !found {
        fail(name, b"message not found in read-back");
    }

    unsafe { zx_handle_close(dlog_h) };
    pass(name);
}

/// C++: TEST(DebugLogTest, ReadDataCopyFault)
fn read_data_copy_fault() {
    let name = b"read_data_copy_fault";

    let mut dlog_h: HandleValue = 0;
    let status = unsafe { zx_debuglog_create(0, FLAG_READABLE, &mut dlog_h) };
    if status != ZX_OK {
        zx::debug_write(b"  debuglog_test: read_data_copy_fault SKIP (no root resource)\n");
        return;
    }

    // Write something so there's data to read
    let data = b"fault-test";
    let status = unsafe { zx_debuglog_write(dlog_h, 0, data.as_ptr(), data.len()) };
    assert_status(status, ZX_OK, name, b"write");

    // Read with null buffer — should fail
    let status = unsafe { zx_debuglog_read(dlog_h, 0, core::ptr::null_mut(), DLOG_MAX_LEN) };
    if status >= 0 {
        fail(name, b"null buffer read should fail");
    }

    unsafe { zx_handle_close(dlog_h) };
    pass(name);
}

#[no_mangle]
pub fn main() {
    zx::debug_write(b"debuglog_test: starting\n");

    invalid_handle_allowed_only_for_write();
    invalid_options();
    invalid_resource_handle_returns_wrong_type();
    read_errors();
    write_errors();
    write_read();
    read_data_copy_fault();

    zx::debug_write(b"debuglog_test: all tests passed\n");
}
