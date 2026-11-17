//! petal VMO transfer_data test -- exercises zx_vmo_transfer_data from user mode.
//!
//! Port of zircon-tests/src/core_tests/syscall_transfer_data.rs.

#![no_std]
#![no_main]
#![allow(dead_code)]

extern crate petal;

use zx::sys::*;

fn pass(name: &[u8]) {
    zx::debug_write(b"  transfer_data_test: ");
    zx::debug_write(name);
    zx::debug_write(b" PASS\n");
}

fn fail(name: &[u8], msg: &[u8]) {
    zx::debug_write(b"FAIL: transfer_data_test: ");
    zx::debug_write(name);
    zx::debug_write(b" - ");
    zx::debug_write(msg);
    zx::debug_write(b"\n");
    zx::Process::exit(1);
}

fn write_i32(val: i32) {
    if val < 0 {
        zx::debug_write(b"-");
        write_u32((-val) as u32);
    } else {
        write_u32(val as u32);
    }
}

fn write_u32(mut val: u32) {
    if val == 0 {
        zx::debug_write(b"0");
        return;
    }
    let mut buf = [0u8; 10];
    let mut i = buf.len();
    while val > 0 {
        i -= 1;
        buf[i] = b'0' + (val % 10) as u8;
        val /= 10;
    }
    zx::debug_write(&buf[i..]);
}

fn assert_status(got: ZxStatus, expected: ZxStatus, name: &[u8], msg: &[u8]) {
    if got != expected {
        zx::debug_write(b"  expected ");
        write_i32(expected);
        zx::debug_write(b" (");
        zx::debug_write(status_name(expected));
        zx::debug_write(b") got ");
        write_i32(got);
        zx::debug_write(b" (");
        zx::debug_write(status_name(got));
        zx::debug_write(b")\n");
        fail(name, msg);
    }
}

fn status_name(s: ZxStatus) -> &'static [u8] {
    match s {
        0 => b"OK",
        -1 => b"INTERNAL",
        -2 => b"NOT_SUPPORTED",
        -4 => b"NO_MEMORY",
        -10 => b"INVALID_ARGS",
        -11 => b"BAD_HANDLE",
        -20 => b"BAD_STATE",
        -21 => b"TIMED_OUT",
        -22 => b"SHOULD_WAIT",
        -24 => b"NOT_FOUND",
        -30 => b"ACCESS_DENIED",
        -35 => b"OUT_OF_RANGE",
        -38 => b"BUFFER_TOO_SMALL",
        -54 => b"WRONG_TYPE",
        _ => b"UNKNOWN_STATUS",
    }
}

fn vmo_create(size: u64) -> HandleValue {
    let mut h: HandleValue = 0;
    let s = unsafe { zx_vmo_create(size, 0, &mut h) };
    if s != ZX_OK {
        fail(b"vmo_create", b"failed");
    }
    h
}

/// C++: TEST(VmoTransferDataTestCase, InvalidInputs)
fn invalid_inputs() {
    let name = b"invalid_inputs";
    let page_size = zx_system_get_page_size() as u64;
    let src = vmo_create(5 * page_size);
    let dst = vmo_create(5 * page_size);

    // Non-zero options -> INVALID_ARGS
    assert_status(
        unsafe { zx_vmo_transfer_data(dst, 1, 0, page_size, src, 0) },
        ZX_ERR_INVALID_ARGS,
        name,
        b"non-zero options",
    );

    // Non-aligned dst offset -> INVALID_ARGS
    assert_status(
        unsafe { zx_vmo_transfer_data(dst, 0, 1, page_size, src, 0) },
        ZX_ERR_INVALID_ARGS,
        name,
        b"non-aligned dst offset",
    );

    // Non-aligned src offset -> INVALID_ARGS
    assert_status(
        unsafe { zx_vmo_transfer_data(dst, 0, 0, page_size, src, 1) },
        ZX_ERR_INVALID_ARGS,
        name,
        b"non-aligned src offset",
    );

    // Non-aligned length -> INVALID_ARGS
    assert_status(
        unsafe { zx_vmo_transfer_data(dst, 0, 0, 1, src, 0) },
        ZX_ERR_INVALID_ARGS,
        name,
        b"non-aligned length",
    );

    // Zero length -> OK (no-op)
    let s = unsafe { zx_vmo_transfer_data(dst, 0, 0, 0, src, 0) };
    if s != ZX_OK {
        fail(name, b"zero length should succeed");
    }

    // Out-of-range dst offset
    assert_status(
        unsafe { zx_vmo_transfer_data(dst, 0, 5 * page_size, page_size, src, 0) },
        ZX_ERR_OUT_OF_RANGE,
        name,
        b"dst offset past end",
    );

    // Out-of-range src offset
    assert_status(
        unsafe { zx_vmo_transfer_data(dst, 0, 0, page_size, src, 5 * page_size) },
        ZX_ERR_OUT_OF_RANGE,
        name,
        b"src offset past end",
    );

    // Bad handle
    assert_status(
        unsafe { zx_vmo_transfer_data(0xDEAD, 0, 0, page_size, src, 0) },
        ZX_ERR_BAD_HANDLE,
        name,
        b"bad dst handle",
    );
    assert_status(
        unsafe { zx_vmo_transfer_data(dst, 0, 0, page_size, 0xDEAD, 0) },
        ZX_ERR_BAD_HANDLE,
        name,
        b"bad src handle",
    );

    unsafe { zx_handle_close(src) };
    unsafe { zx_handle_close(dst) };
    pass(name);
}

#[no_mangle]
pub fn main() {
    zx::debug_write(b"transfer_data_test: starting\n");
    invalid_inputs();
    zx::debug_write(b"transfer_data_test: all tests passed\n");
}
