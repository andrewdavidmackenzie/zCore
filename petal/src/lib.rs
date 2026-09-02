//! petal runtime -- provides _start entry point and panic handler.
//!
//! Petal programs just define `fn main()` and this runtime handles
//! the boilerplate: entry point, calling main, exiting the process,
//! and panic handling.

#![no_std]

use zircon_abi::syscall;

extern "Rust" {
    /// The user's main function. Defined in the binary crate.
    fn main();
}

/// Entry point -- called by the kernel when the process starts.
/// Sets up the environment, calls main(), then exits.
#[no_mangle]
pub extern "C" fn _start() -> ! {
    unsafe { main() };
    syscall::process_exit(0);
}

/// Panic handler -- writes a message and exits with code 1.
#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    syscall::debug_write(b"petal: PANIC!\n");
    syscall::process_exit(1);
}
