//! petal hello world -- a minimal Zircon userspace program.

#![no_std]
#![no_main]

extern crate petal; // links the runtime

use zircon_abi::syscall;

#[no_mangle]
pub fn main() {
    syscall::debug_print("petal: Hello from petal on zCore!\n");
}
