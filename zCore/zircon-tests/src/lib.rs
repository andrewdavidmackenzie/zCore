//! Rust-native Zircon syscall test suite.
//!
//! This crate tests Zircon kernel syscalls through the full dispatch
//! path (zircon-syscall → zircon-object) in libos mode. Tests run on
//! the host with `cargo test -p zircon-tests`, without QEMU.
//!
//! # Architecture
//!
//! Each test creates a minimal Zircon process context using
//! [`test_setup()`], then dispatches syscalls via the
//! [`zircon_syscall::Syscall`] struct. This exercises the same
//! kernel code path as real Zircon userspace programs.

#[cfg(test)]
mod helpers;

#[cfg(test)]
mod channel;

#[cfg(test)]
mod vmo;
