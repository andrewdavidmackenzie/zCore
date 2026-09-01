//! petal hello world -- a minimal Zircon userspace program.
//!
//! This program runs on the zCore Zircon kernel. It uses inline syscall
//! wrappers from `zircon-abi` to call `zx_debug_write` (print a message)
//! and `zx_process_exit` (exit cleanly).
//!
//! Build: cross-compiled as a freestanding aarch64 binary, then stripped
//! to a flat binary and packaged into a ZBI for the kernel to load.

#![no_std]
#![no_main]

use core::panic::PanicInfo;
use zircon_abi::syscall;

/// Entry point -- called by the kernel when the process starts.
#[no_mangle]
pub extern "C" fn _start() -> ! {
    let msg = b"userstart: Hello from petal on zCore!\n";
    unsafe {
        syscall::zx_debug_write(msg.as_ptr(), msg.len());
        syscall::zx_process_exit(0);
    }
}

/// Panic handler -- required for #![no_std].
#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    // Write panic message via debug_write if possible
    let msg = b"petal: PANIC!\n";
    unsafe {
        syscall::zx_debug_write(msg.as_ptr(), msg.len());
        syscall::zx_process_exit(1);
    }
}
