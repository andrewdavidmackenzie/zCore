//! petal hello world -- a minimal Zircon userspace program.
//!
//! This program runs on the zCore Zircon kernel. The petal runtime
//! (in lib.rs) provides _start and the panic handler. This file
//! just defines main(), like a normal Rust program.

#![no_std]
#![no_main]

// Link the petal runtime (provides _start and panic handler)
extern crate petal;

use zircon_abi::syscall;

/// Main function -- called by the petal runtime's _start.
#[no_mangle]
fn main() {
    let msg = b"petal: Hello from petal on zCore!\n";
    unsafe {
        syscall::zx_debug_write(msg.as_ptr(), msg.len());
    }
    // Returning from main() causes the runtime to call process_exit(0)
}
