//! petal CPRNG test -- exercises cprng_draw_once and cprng_add_entropy.
//!
//! Port of syscall-layer tests from zircon-tests/src/cprng.rs.

#![no_std]
#![no_main]
#![allow(dead_code)]

extern crate petal;

use zx::sys::*;

fn pass(name: &[u8]) {
    zx::debug_write(b"  cprng_test: ");
    zx::debug_write(name);
    zx::debug_write(b" PASS\n");
}

fn fail(name: &[u8], msg: &[u8]) {
    zx::debug_write(b"FAIL: cprng_test: ");
    zx::debug_write(name);
    zx::debug_write(b" - ");
    zx::debug_write(msg);
    zx::debug_write(b"\n");
    zx::Process::exit(1);
}

/// Draw random bytes and verify non-zero
fn draw_non_zero() {
    let name = b"draw_non_zero";
    let mut buf = [0u8; 32];
    let s = unsafe { zx_cprng_draw_once(buf.as_mut_ptr(), 32) };
    if s != ZX_OK {
        fail(name, b"draw failed");
    }
    if !buf.iter().any(|&b| b != 0) {
        fail(name, b"all zeros");
    }
    pass(name);
}

/// Draw two buffers and verify they differ
fn draw_different() {
    let name = b"draw_different";
    let mut buf1 = [0u8; 32];
    let mut buf2 = [0u8; 32];
    let s1 = unsafe { zx_cprng_draw_once(buf1.as_mut_ptr(), 32) };
    let s2 = unsafe { zx_cprng_draw_once(buf2.as_mut_ptr(), 32) };
    if s1 != ZX_OK || s2 != ZX_OK {
        fail(name, b"draw failed");
    }
    if buf1 == buf2 {
        fail(name, b"buffers should differ");
    }
    pass(name);
}

/// Draw zero bytes — should succeed
fn draw_zero_length() {
    let name = b"draw_zero_length";
    let s = unsafe { zx_cprng_draw_once(core::ptr::null_mut(), 0) };
    if s != ZX_OK {
        fail(name, b"zero length draw should succeed");
    }
    pass(name);
}

/// Draw max length (256 bytes)
fn draw_max_length() {
    let name = b"draw_max_length";
    let mut buf = [0u8; 256];
    let s = unsafe { zx_cprng_draw_once(buf.as_mut_ptr(), 256) };
    if s != ZX_OK {
        fail(name, b"max length draw failed");
    }
    if !buf.iter().any(|&b| b != 0) {
        fail(name, b"all zeros");
    }
    pass(name);
}

/// Draw > 256 bytes — should fail
fn draw_exceeds_max_length() {
    let name = b"draw_exceeds_max_length";
    let mut buf = [0u8; 257];
    let s = unsafe { zx_cprng_draw_once(buf.as_mut_ptr(), 257) };
    if s == ZX_OK {
        fail(name, b"should fail for > 256");
    }
    pass(name);
}

/// Draw with null buffer — should fail
fn draw_null_buffer() {
    let name = b"draw_null_buffer";
    let s = unsafe { zx_cprng_draw_once(core::ptr::null_mut(), 16) };
    if s == ZX_OK {
        fail(name, b"null buffer should fail");
    }
    pass(name);
}

/// Add entropy from valid buffer
fn add_entropy_valid() {
    let name = b"add_entropy_valid";
    let entropy = [0x42u8; 32];
    let s = unsafe { zx_cprng_add_entropy(entropy.as_ptr(), entropy.len()) };
    if s != ZX_OK {
        fail(name, b"add entropy failed");
    }
    pass(name);
}

/// Add entropy with null buffer — should fail
fn add_entropy_null_buffer() {
    let name = b"add_entropy_null_buffer";
    let s = unsafe { zx_cprng_add_entropy(core::ptr::null(), 16) };
    if s == ZX_OK {
        fail(name, b"null buffer should fail");
    }
    pass(name);
}

/// Add zero-length entropy — should succeed
fn add_entropy_zero_length() {
    let name = b"add_entropy_zero_length";
    let s = unsafe { zx_cprng_add_entropy(core::ptr::null(), 0) };
    if s != ZX_OK {
        fail(name, b"zero length should succeed");
    }
    pass(name);
}

#[no_mangle]
pub fn main() {
    zx::debug_write(b"cprng_test: starting\n");

    draw_non_zero();
    draw_different();
    draw_zero_length();
    draw_max_length();
    draw_exceeds_max_length();
    draw_null_buffer();
    add_entropy_valid();
    add_entropy_null_buffer();
    add_entropy_zero_length();

    zx::debug_write(b"cprng_test: all tests passed\n");
}
