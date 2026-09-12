//! petal runtime -- provides _start entry point and panic handler.
//!
//! Petal programs define `pub fn main()` and this runtime handles
//! the boilerplate. The startup handle from userstart is available
//! via `petal::take_startup_handle()`.

#![no_std]

use core::sync::atomic::{AtomicU32, AtomicUsize, Ordering};

extern "Rust" {
    /// The user's main function.
    fn main();
}

static STARTUP_HANDLE: AtomicU32 = AtomicU32::new(0);
static VDSO_BASE: AtomicUsize = AtomicUsize::new(0);

/// Take the startup handle passed by userstart.
/// Returns 0 (ZX_HANDLE_INVALID) if already taken or not set.
pub fn take_startup_handle() -> u32 {
    STARTUP_HANDLE.swap(0, Ordering::SeqCst)
}

/// Get the vDSO code base address passed by the kernel.
/// Returns 0 if not set.
pub fn vdso_base() -> usize {
    VDSO_BASE.load(Ordering::SeqCst)
}

/// Entry point -- called by the kernel when the process starts.
/// `startup_handle`: bootstrap channel handle
/// `vdso_base`: base address of the vDSO code mapping
#[no_mangle]
pub extern "C" fn _start(startup_handle: u32, vdso_base: usize) -> ! {
    STARTUP_HANDLE.store(startup_handle, Ordering::SeqCst);
    VDSO_BASE.store(vdso_base, Ordering::SeqCst);
    unsafe { main() };
    zx::Process::exit(0);
}

/// Panic handler -- writes a message and exits with code 1.
#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    zx::debug_write(b"petal: PANIC!\n");
    zx::Process::exit(1);
}
