//! petal channel test -- exercises Zircon channel syscalls.
//!
//! Tests: channel_create, channel_write, channel_read, handle_close.

#![no_std]
#![no_main]

extern crate petal;

use zircon_abi::errors::*;
use zircon_abi::syscall;

#[no_mangle]
pub fn main() {
    syscall::debug_print("channel_test: starting\n");

    // Create a channel pair
    let mut ch0: u32 = 0;
    let mut ch1: u32 = 0;
    let status = unsafe { syscall::zx_channel_create(0, &mut ch0, &mut ch1) };
    assert_ok("channel_create", status);
    syscall::debug_print("channel_test: channel created\n");

    // Write a message
    let msg = b"hello channel!";
    let status = unsafe {
        syscall::zx_channel_write(ch0, 0, msg.as_ptr(), msg.len() as u32, core::ptr::null(), 0)
    };
    assert_ok("channel_write", status);
    syscall::debug_print("channel_test: message written\n");

    // Read it back from the other end
    let mut buf = [0u8; 64];
    let mut actual_bytes: u32 = 0;
    let mut actual_handles: u32 = 0;
    let status = unsafe {
        syscall::zx_channel_read(
            ch1,
            0,
            buf.as_mut_ptr(),
            core::ptr::null_mut(),
            buf.len() as u32,
            0,
            &mut actual_bytes,
            &mut actual_handles,
        )
    };
    assert_ok("channel_read", status);

    if actual_bytes as usize == msg.len() && &buf[..msg.len()] == msg {
        syscall::debug_print("channel_test: message verified\n");
    } else {
        syscall::debug_print("channel_test: FAIL - message mismatch\n");
        syscall::process_exit(1);
    }

    // Close both ends
    unsafe {
        syscall::zx_handle_close(ch0);
        syscall::zx_handle_close(ch1);
    }

    syscall::debug_print("channel_test: PASS\n");
}

fn assert_ok(name: &str, status: ZxStatus) {
    if status != ZX_OK {
        syscall::debug_print("channel_test: FAIL - ");
        syscall::debug_print(name);
        syscall::debug_print(" failed\n");
        syscall::process_exit(1);
    }
}
