//! petal hello world -- a minimal Zircon userspace program.
//!
//! The petal runtime (lib.rs) provides the entry point (_start) and
//! panic handler. Programs just define main().
//!
//! Note: #![no_main] and #[no_mangle] are required boilerplate for
//! #![no_std] freestanding binaries. The petal runtime calls main().

#![no_std]
#![no_main]

extern crate petal; // links the runtime

use zircon_abi::syscall;

#[no_mangle]
fn main() {
    syscall::debug_print("petal: Hello from petal on zCore!\n");
}
