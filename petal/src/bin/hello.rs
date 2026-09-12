//! petal hello world -- a minimal Zircon userspace program.

#![no_std]
#![no_main]

extern crate petal; // links the runtime

#[no_mangle]
pub fn main() {
    zx::debug_write(b"petal: Hello from petal on zCore!\n");
}
