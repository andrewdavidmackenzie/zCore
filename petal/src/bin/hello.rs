//! petal hello world -- a minimal Zircon userspace program.
//!
//! Demonstrates:
//! - Receiving the startup channel handle from userstart
//! - Reading bootstrap handles from the channel
//! - Writing a debug message via zx_debug_write

#![no_std]
#![no_main]

extern crate petal; // links the runtime

use zircon_abi::errors::*;
use zircon_abi::syscall;

#[no_mangle]
pub fn main(startup_handle: u32) {
    syscall::debug_print("petal: Hello from petal on zCore!\n");

    if startup_handle == 0 {
        syscall::debug_print("petal: no startup handle received\n");
        return;
    }

    // Read bootstrap handles from the channel
    let mut handles = [0u32; 8];
    let mut actual_bytes: u32 = 0;
    let mut actual_handles: u32 = 0;
    let status = unsafe {
        syscall::zx_channel_read(
            startup_handle,
            0,
            core::ptr::null_mut(),
            handles.as_mut_ptr(),
            0,
            handles.len() as u32,
            &mut actual_bytes,
            &mut actual_handles,
        )
    };

    if status == ZX_OK && actual_handles > 0 {
        syscall::debug_print("petal: received bootstrap handles from userstart\n");

        // Close the handles we received
        for &h in handles.iter().take(actual_handles as usize) {
            if h != 0 {
                unsafe { syscall::zx_handle_close(h) };
            }
        }
    } else {
        syscall::debug_print("petal: failed to read bootstrap handles\n");
    }

    // Close the startup channel
    unsafe { syscall::zx_handle_close(startup_handle) };

    syscall::debug_print("petal: done\n");
}
