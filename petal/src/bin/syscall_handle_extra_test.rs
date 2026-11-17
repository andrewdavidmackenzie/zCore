//! petal handle extra test -- exercises handle close/dup/replace syscalls.
//!
//! Port of zircon-tests/src/syscall_handle_extra.rs.

#![no_std]
#![no_main]
#![allow(dead_code)]

extern crate petal;

use zx::sys::*;

fn pass(name: &[u8]) {
    zx::debug_write(b"  handle_extra_test: ");
    zx::debug_write(name);
    zx::debug_write(b" PASS\n");
}

fn fail(name: &[u8], msg: &[u8]) {
    zx::debug_write(b"FAIL: handle_extra_test: ");
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

fn event_create() -> HandleValue {
    let mut h: HandleValue = 0;
    let s = unsafe { zx_event_create(0, &mut h) };
    if s != ZX_OK {
        fail(b"event_create", b"failed");
    }
    h
}

/// Close via syscall
fn close_via_syscall() {
    let name = b"close_via_syscall";
    let h = event_create();
    assert_ok(unsafe { zx_handle_close(h) }, name, b"close");
    // Double close should fail
    assert_status(
        unsafe { zx_handle_close(h) },
        ZX_ERR_BAD_HANDLE,
        name,
        b"double close",
    );
    pass(name);
}

/// Close invalid handle
fn close_invalid_handle() {
    let name = b"close_invalid_handle";
    assert_status(
        unsafe { zx_handle_close(0xDEAD) },
        ZX_ERR_BAD_HANDLE,
        name,
        b"",
    );
    pass(name);
}

/// Duplicate via syscall
fn duplicate_via_syscall() {
    let name = b"duplicate_via_syscall";
    let h = event_create();
    let mut dup: HandleValue = 0;
    assert_ok(
        unsafe { zx_handle_duplicate(h, ZX_RIGHT_SAME_RIGHTS, &mut dup) },
        name,
        b"dup",
    );
    if dup == 0 || dup == h {
        fail(name, b"bad dup handle");
    }
    // Both should be valid
    assert_ok(unsafe { zx_handle_check_valid(h) }, name, b"original valid");
    assert_ok(unsafe { zx_handle_check_valid(dup) }, name, b"dup valid");
    unsafe { zx_handle_close(h) };
    unsafe { zx_handle_close(dup) };
    pass(name);
}

/// Duplicate with reduced rights
fn duplicate_reduced_rights() {
    let name = b"duplicate_reduced_rights";
    let h = event_create();
    let mut dup: HandleValue = 0;
    assert_ok(
        unsafe { zx_handle_duplicate(h, ZX_RIGHT_SIGNAL | ZX_RIGHT_WAIT, &mut dup) },
        name,
        b"dup reduced",
    );
    if dup == 0 {
        fail(name, b"dup handle is 0");
    }
    // Try to signal (should work — has SIGNAL right)
    assert_ok(
        unsafe {
            zx_object_signal(dup, 0, 1 << 24 /* USER_SIGNAL_0 */)
        },
        name,
        b"signal from reduced",
    );
    unsafe { zx_handle_close(h) };
    unsafe { zx_handle_close(dup) };
    pass(name);
}

/// Replace via syscall
fn replace_via_syscall() {
    let name = b"replace_via_syscall";
    let h = event_create();
    let mut replaced: HandleValue = 0;
    assert_ok(
        unsafe { zx_handle_replace(h, ZX_RIGHT_SAME_RIGHTS, &mut replaced) },
        name,
        b"replace",
    );
    if replaced == 0 {
        fail(name, b"replaced handle is 0");
    }
    // Original should be invalid
    assert_status(
        unsafe { zx_handle_check_valid(h) },
        ZX_ERR_BAD_HANDLE,
        name,
        b"original after replace",
    );
    // Replacement should be valid
    assert_ok(
        unsafe { zx_handle_check_valid(replaced) },
        name,
        b"replaced valid",
    );
    unsafe { zx_handle_close(replaced) };
    pass(name);
}

/// Duplicate expanded rights fails
fn duplicate_expanded_rights_fails() {
    let name = b"duplicate_expanded_rights_fails";
    let h = event_create();
    // Dup with just WAIT
    let mut limited: HandleValue = 0;
    assert_ok(
        unsafe { zx_handle_duplicate(h, ZX_RIGHT_WAIT | ZX_RIGHT_INSPECT, &mut limited) },
        name,
        b"dup limited",
    );
    // Try to dup again with SIGNAL (escalation) — should fail
    let mut escalated: HandleValue = 0;
    let s =
        unsafe { zx_handle_duplicate(limited, ZX_RIGHT_SIGNAL | ZX_RIGHT_WAIT, &mut escalated) };
    if s == ZX_OK {
        fail(name, b"escalation should fail");
    }
    unsafe { zx_handle_close(limited) };
    unsafe { zx_handle_close(h) };
    pass(name);
}

/// Duplicate requires DUPLICATE right
fn duplicate_requires_duplicate_right() {
    let name = b"duplicate_requires_dup_right";
    let h = event_create();
    // Dup without DUPLICATE right
    let mut no_dup: HandleValue = 0;
    assert_ok(
        unsafe {
            zx_handle_duplicate(
                h,
                ZX_RIGHT_SIGNAL | ZX_RIGHT_WAIT | ZX_RIGHT_INSPECT,
                &mut no_dup,
            )
        },
        name,
        b"dup without DUP right",
    );
    // Try to dup again — should fail (no DUPLICATE right)
    let mut dup2: HandleValue = 0;
    let s = unsafe { zx_handle_duplicate(no_dup, ZX_RIGHT_SAME_RIGHTS, &mut dup2) };
    if s == ZX_OK {
        fail(name, b"dup without DUP right should fail");
    }
    unsafe { zx_handle_close(no_dup) };
    unsafe { zx_handle_close(h) };
    pass(name);
}

/// Duplicate invalid handle
fn duplicate_invalid_handle_syscall() {
    let name = b"duplicate_invalid_handle";
    let mut dup: HandleValue = 0;
    assert_status(
        unsafe { zx_handle_duplicate(0xDEAD, ZX_RIGHT_SAME_RIGHTS, &mut dup) },
        ZX_ERR_BAD_HANDLE,
        name,
        b"",
    );
    pass(name);
}

/// Duplicate same rights
fn duplicate_same_rights_syscall() {
    let name = b"duplicate_same_rights";
    let h = event_create();
    let mut dup: HandleValue = 0;
    assert_ok(
        unsafe { zx_handle_duplicate(h, ZX_RIGHT_SAME_RIGHTS, &mut dup) },
        name,
        b"dup",
    );
    // Check both have same type (event = 5)
    let mut buf = [0u8; 32];
    let mut actual: usize = 0;
    let mut avail: usize = 0;
    assert_ok(
        unsafe {
            zx_object_get_info(
                dup,
                2, /* HANDLE_BASIC */
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
    if obj_type != 5 {
        fail(name, b"wrong obj_type (expected 5=Event)");
    }
    unsafe { zx_handle_close(h) };
    unsafe { zx_handle_close(dup) };
    pass(name);
}

/// Replace bad handle
fn replace_bad_handle_syscall() {
    let name = b"replace_bad_handle";
    let mut out: HandleValue = 0;
    assert_status(
        unsafe { zx_handle_replace(0xDEAD, ZX_RIGHT_SAME_RIGHTS, &mut out) },
        ZX_ERR_BAD_HANDLE,
        name,
        b"",
    );
    pass(name);
}

#[no_mangle]
pub fn main() {
    zx::debug_write(b"handle_extra_test: starting\n");

    close_via_syscall();
    close_invalid_handle();
    duplicate_via_syscall();
    duplicate_reduced_rights();
    replace_via_syscall();
    duplicate_expanded_rights_fails();
    duplicate_requires_duplicate_right();
    duplicate_invalid_handle_syscall();
    duplicate_same_rights_syscall();
    replace_bad_handle_syscall();

    zx::debug_write(b"handle_extra_test: all tests passed\n");
}
