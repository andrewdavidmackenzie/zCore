//! petal bad_syscall test -- verify kernel rejects invalid syscall args.
//!
//! Port of zircon-tests/src/bad_syscall.rs.

#![no_std]
#![no_main]
#![allow(dead_code)]

extern crate petal;

use zx::sys::*;

fn pass(name: &[u8]) {
    zx::debug_write(b"  bad_syscall_test: ");
    zx::debug_write(name);
    zx::debug_write(b" PASS\n");
}

fn fail(name: &[u8], msg: &[u8]) {
    zx::debug_write(b"FAIL: bad_syscall_test: ");
    zx::debug_write(name);
    zx::debug_write(b" - ");
    zx::debug_write(msg);
    zx::debug_write(b"\n");
    zx::Process::exit(1);
}

/// Bad handle value should return BAD_HANDLE
fn bad_syscall_invalid_number() {
    let name = b"invalid_number";
    let s = unsafe { zx_handle_close(0xBAD_CAFE) };
    if s != ZX_ERR_BAD_HANDLE {
        fail(name, b"expected BAD_HANDLE");
    }
    pass(name);
}

/// Bad handle in various syscalls
fn bad_syscall_invalid_handle() {
    let name = b"invalid_handle";
    let bad_h: HandleValue = 0xDEAD_BEEF;

    let s = unsafe { zx_handle_close(bad_h) };
    if s != ZX_ERR_BAD_HANDLE {
        fail(name, b"close bad handle");
    }

    let mut out: HandleValue = 0;
    let s = unsafe { zx_handle_duplicate(bad_h, 0, &mut out) };
    if s != ZX_ERR_BAD_HANDLE {
        fail(name, b"dup bad handle");
    }

    pass(name);
}

/// Handle 0 (ZX_HANDLE_INVALID) — close is no-op, dup fails
fn bad_syscall_zero_handle() {
    let name = b"zero_handle";

    // Close ZX_HANDLE_INVALID is a no-op
    let s = unsafe { zx_handle_close(0) };
    if s != ZX_OK {
        fail(name, b"close 0 should succeed");
    }

    // Dup ZX_HANDLE_INVALID should fail
    let mut out: HandleValue = 0;
    let s = unsafe { zx_handle_duplicate(0, 0, &mut out) };
    if s != ZX_ERR_BAD_HANDLE {
        fail(name, b"dup 0 should fail");
    }

    pass(name);
}

/// u32::MAX as handle
fn bad_syscall_negative_handle() {
    let name = b"negative_handle";
    let s = unsafe { zx_handle_close(u32::MAX) };
    if s != ZX_ERR_BAD_HANDLE {
        fail(name, b"expected BAD_HANDLE");
    }
    pass(name);
}

#[no_mangle]
pub fn main() {
    zx::debug_write(b"bad_syscall_test: starting\n");
    bad_syscall_invalid_number();
    bad_syscall_invalid_handle();
    bad_syscall_zero_handle();
    bad_syscall_negative_handle();
    zx::debug_write(b"bad_syscall_test: all tests passed\n");
}
