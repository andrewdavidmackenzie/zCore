//! petal runtime -- provides _start entry point and panic handler.
//!
//! Petal programs define `pub fn main(startup_handle: u32)` and this
//! runtime handles the boilerplate: entry point, calling main with
//! the startup handle, exiting the process, and panic handling.
//!
//! The startup handle is a channel received from userstart containing
//! the bootstrap handles (root job, ZBI VMO, etc.).

#![no_std]

use zircon_abi::syscall;

extern "Rust" {
    /// The user's main function. Receives the startup channel handle.
    fn main(startup_handle: u32);
}

/// Entry point -- called by the kernel when the process starts.
/// The startup handle is passed in the first argument register.
#[no_mangle]
pub extern "C" fn _start(startup_handle: u32, _arg2: usize) -> ! {
    unsafe { main(startup_handle) };
    syscall::process_exit(0);
}

/// Panic handler -- writes a message and exits with code 1.
#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    syscall::debug_write(b"petal: PANIC!\n");
    syscall::process_exit(1);
}
