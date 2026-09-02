//! petal runtime -- provides _start entry point and panic handler.
//!
//! Petal programs define `pub fn main()` and this runtime handles
//! the boilerplate. The startup handle from userstart is available
//! via `petal::take_startup_handle()`.

#![no_std]

use core::sync::atomic::{AtomicU32, Ordering};
use zircon_abi::syscall;

extern "Rust" {
    /// The user's main function.
    fn main();
}

static STARTUP_HANDLE: AtomicU32 = AtomicU32::new(0);

/// Take the startup handle passed by userstart.
/// Returns 0 (ZX_HANDLE_INVALID) if already taken or not set.
pub fn take_startup_handle() -> u32 {
    STARTUP_HANDLE.swap(0, Ordering::SeqCst)
}

/// Entry point -- called by the kernel when the process starts.
#[no_mangle]
pub extern "C" fn _start(startup_handle: u32, _arg2: usize) -> ! {
    STARTUP_HANDLE.store(startup_handle, Ordering::SeqCst);
    unsafe { main() };
    syscall::process_exit(0);
}

/// Panic handler -- writes a message and exits with code 1.
#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    syscall::debug_write(b"petal: PANIC!\n");
    syscall::process_exit(1);
}
