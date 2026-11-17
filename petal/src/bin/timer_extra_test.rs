//! petal timer extra test -- timer create with invalid options.
//!
//! Port of UserPtr test from zircon-tests/src/timer_extra.rs.

#![no_std]
#![no_main]
#![allow(dead_code)]

extern crate petal;

use zx::sys::*;

fn pass(name: &[u8]) {
    zx::debug_write(b"  timer_extra_test: ");
    zx::debug_write(name);
    zx::debug_write(b" PASS\n");
}

fn fail(name: &[u8], msg: &[u8]) {
    zx::debug_write(b"FAIL: timer_extra_test: ");
    zx::debug_write(name);
    zx::debug_write(b" - ");
    zx::debug_write(msg);
    zx::debug_write(b"\n");
    zx::Process::exit(1);
}

/// Invalid options for timer create
fn timer_invalid_args() {
    let name = b"timer_invalid_args";
    let mut h: HandleValue = 0;

    // clock_id=3 is invalid
    let s = unsafe { zx_timer_create(3, 0, &mut h) };
    if s == ZX_OK {
        unsafe { zx_handle_close(h) };
        fail(name, b"clock_id 3 should fail");
    }

    // options=0xFF is invalid
    let s = unsafe { zx_timer_create(0xFF, 0, &mut h) };
    if s == ZX_OK {
        unsafe { zx_handle_close(h) };
        fail(name, b"options 0xFF should fail");
    }

    pass(name);
}

#[no_mangle]
pub fn main() {
    zx::debug_write(b"timer_extra_test: starting\n");
    timer_invalid_args();
    zx::debug_write(b"timer_extra_test: all tests passed\n");
}
