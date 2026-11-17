//! petal handle syscall test -- exercises handle close/duplicate/replace.
//!
//! Port of zircon-tests/src/core_tests/syscall_handle.rs.

#![no_std]
#![no_main]
#![allow(dead_code)]

extern crate alloc;
extern crate petal;

use alloc::vec::Vec;
use zx::sys::*;

fn pass(name: &[u8]) {
    zx::debug_write(b"  syscall_handle_test: ");
    zx::debug_write(name);
    zx::debug_write(b" PASS\n");
}

fn fail(name: &[u8], msg: &[u8]) {
    zx::debug_write(b"FAIL: syscall_handle_test: ");
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

/// C++: TEST(HandleCloseTest, Many)
fn close_many_handles() {
    let name = b"close_many_handles";

    let mut handles = Vec::new();
    for _ in 0..5 {
        let mut h0: HandleValue = 0;
        let mut h1: HandleValue = 0;
        assert_ok(
            unsafe { zx_eventpair_create(0, &mut h0, &mut h1) },
            name,
            b"eventpair create",
        );
        handles.push(h0);
        handles.push(h1);
    }

    // Close all at once
    assert_ok(
        unsafe { zx_handle_close_many(handles.as_ptr(), handles.len()) },
        name,
        b"close_many",
    );

    // All should be gone
    for &h in &handles {
        let s = unsafe { zx_handle_check_valid(h) };
        // Should return NOT_FOUND or BAD_HANDLE
        if s == ZX_OK {
            fail(name, b"handle still valid after close_many");
        }
    }

    pass(name);
}

/// C++: TEST(HandleDup, ReplaceSuccessOrigInvalid)
fn replace_success_orig_invalid() {
    let name = b"replace_success_orig_invalid";

    let mut h: HandleValue = 0;
    assert_ok(unsafe { zx_event_create(0, &mut h) }, name, b"event create");

    let mut h2: HandleValue = 0;
    assert_ok(
        unsafe { zx_handle_replace(h, ZX_RIGHT_SAME_RIGHTS, &mut h2) },
        name,
        b"replace",
    );
    if h2 == 0 {
        fail(name, b"replacement handle is 0");
    }

    // Original should be invalid
    let s = unsafe { zx_handle_check_valid(h) };
    if s == ZX_OK {
        fail(name, b"original should be invalid after replace");
    }

    // New handle should be valid
    assert_ok(
        unsafe { zx_handle_check_valid(h2) },
        name,
        b"new handle invalid",
    );

    unsafe { zx_handle_close(h2) };
    pass(name);
}

/// C++: TEST(HandleDup, ReplaceFailureBothInvalid)
fn replace_failure_both_invalid() {
    let name = b"replace_failure_both_invalid";

    // Create event, duplicate with READ only
    let mut h: HandleValue = 0;
    assert_ok(unsafe { zx_event_create(0, &mut h) }, name, b"event create");

    let mut read_only: HandleValue = 0;
    assert_ok(
        unsafe { zx_handle_duplicate(h, ZX_RIGHT_READ, &mut read_only) },
        name,
        b"duplicate with READ",
    );
    unsafe { zx_handle_close(h) };

    // Try to replace with rights that include WRITE (escalation) — should fail
    let mut h2: HandleValue = 0;
    let s = unsafe { zx_handle_replace(read_only, ZX_RIGHT_READ | ZX_RIGHT_WRITE, &mut h2) };
    if s == ZX_OK {
        fail(name, b"escalation should fail");
    }

    // In zCore, replace does NOT consume the source on failure
    assert_ok(
        unsafe { zx_handle_check_valid(read_only) },
        name,
        b"source still valid",
    );

    unsafe { zx_handle_close(read_only) };
    pass(name);
}

#[no_mangle]
pub fn main() {
    zx::debug_write(b"syscall_handle_test: starting\n");
    close_many_handles();
    replace_success_orig_invalid();
    replace_failure_both_invalid();
    zx::debug_write(b"syscall_handle_test: all tests passed\n");
}
