//! Zircon vDSO implementation.
//!
//! This crate provides the syscall trampolines and system constant
//! accessors that form the vDSO (`libzircon.so`). The vDSO is mapped
//! into every Zircon process by the kernel.
//!
//! Each `zx_*` function is a thin wrapper that loads the syscall
//! number into the appropriate register and executes the trap
//! instruction (`svc #0` on aarch64, `syscall` on x86_64,
//! `ecall` on riscv64).

#![no_std]
#![deny(warnings)]

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {
        core::hint::spin_loop();
    }
}

// Include the generated assembly trampolines.
// The build script generates arch-specific assembly for all syscalls.
include!(concat!(env!("OUT_DIR"), "/trampolines.rs"));
