//! Zircon ABI definitions shared between kernel and userspace.
//!
//! The Zircon kernel defines the syscall interface -- syscall numbers,
//! error codes, handle types, and calling convention. Fuchsia userspace
//! programs depend on this ABI. zCore reimplements the Zircon kernel in
//! Rust, implementing the same syscall ABI so that programs built against
//! Zircon's interface can run on zCore unchanged.
//!
//! This crate defines:
//! - Syscall numbers matching Zircon's `zx-syscall-numbers.h`
//! - Error/status codes matching Zircon's `zx_status_t`
//! - Userspace syscall wrappers (behind the `userspace` feature)
//!
//! Used by `petal` test programs to call Zircon syscalls, and by the
//! kernel to verify ABI compatibility.

#![no_std]
#![deny(warnings)]

#[cfg(feature = "zbi")]
extern crate alloc;

pub mod consts;
pub mod errors;
pub mod zbi;

#[cfg(feature = "userspace")]
pub mod syscall;
