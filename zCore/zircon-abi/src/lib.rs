//! Zircon ABI definitions shared between kernel and userspace.
//!
//! This crate defines:
//! - Syscall numbers matching the Zircon/Fuchsia ABI
//! - Error/status codes (`ZxStatus`)
//! - Userspace syscall wrappers (behind the `userspace` feature)
//!
//! Used by `petal` test programs to call Zircon syscalls, and by the
//! kernel to verify ABI compatibility.

#![no_std]
#![deny(warnings)]

pub mod consts;
pub mod errors;

#[cfg(feature = "userspace")]
pub mod syscall;
