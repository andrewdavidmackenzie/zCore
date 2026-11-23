//! petal hello world -- a minimal Zircon userspace program.
//!
//! Also verifies the bootstrap protocol's argv plumbing (#884): the
//! launcher (kernel → userstart on the ZBI path, or the rootfs spawn
//! path) passes the program name/path as argv[0], per the processargs
//! protocol — the same contract Fuchsia's launchers and userboot
//! honor. The boot tests grep for the printed argv lines.

#![no_std]
#![no_main]

extern crate alloc;
extern crate petal; // links the runtime

#[no_mangle]
pub fn main() {
    zx::debug_write(b"Hello from Zircon on zCore!\r\n");

    if let Some(boot) = petal::Bootstrap::read() {
        let count = boot.arg_count();
        let out = alloc::format!("argc={}\r\n", count);
        zx::debug_write(out.as_bytes());
        for i in 0..count {
            if let Some(arg) = boot.arg(i) {
                let arg = core::str::from_utf8(arg).unwrap_or("<invalid utf8>");
                let out = alloc::format!("argv[{}]=\"{}\"\r\n", i, arg);
                zx::debug_write(out.as_bytes());
            }
        }
    }
}
