//! petal VMO test -- exercises Zircon VMO syscalls.
//!
//! Tests: vmo_create, vmo_write, vmo_read, vmo_get_size, handle_close.

#![no_std]
#![no_main]

extern crate petal;

use zircon_abi::errors::*;
use zircon_abi::syscall;

#[no_mangle]
pub fn main() {
    syscall::debug_print("vmo_test: starting\n");

    // Create a VMO
    let mut vmo: u32 = 0;
    let status = unsafe { syscall::zx_vmo_create(4096, 0, &mut vmo) };
    assert_ok("vmo_create", status);
    syscall::debug_print("vmo_test: VMO created\n");

    // Check size
    let mut size: usize = 0;
    let status = unsafe { syscall::zx_vmo_get_size(vmo, &mut size) };
    assert_ok("vmo_get_size", status);

    if size == 4096 {
        syscall::debug_print("vmo_test: size verified (4096)\n");
    } else {
        syscall::debug_print("vmo_test: FAIL - unexpected size\n");
        syscall::process_exit(1);
    }

    // Write data
    let data = b"Hello from VMO!";
    let status = unsafe { syscall::zx_vmo_write(vmo, data.as_ptr(), 0, data.len()) };
    assert_ok("vmo_write", status);
    syscall::debug_print("vmo_test: data written\n");

    // Read it back
    let mut buf = [0u8; 64];
    let status = unsafe { syscall::zx_vmo_read(vmo, buf.as_mut_ptr(), 0, data.len()) };
    assert_ok("vmo_read", status);

    if &buf[..data.len()] == data {
        syscall::debug_print("vmo_test: data verified\n");
    } else {
        syscall::debug_print("vmo_test: FAIL - data mismatch\n");
        syscall::process_exit(1);
    }

    // Clean up
    unsafe { syscall::zx_handle_close(vmo) };

    syscall::debug_print("vmo_test: PASS\n");
}

fn assert_ok(name: &str, status: ZxStatus) {
    if status != ZX_OK {
        syscall::debug_print("vmo_test: FAIL - ");
        syscall::debug_print(name);
        syscall::debug_print(" failed\n");
        syscall::process_exit(1);
    }
}
