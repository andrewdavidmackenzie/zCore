//! Zircon kernel objects
//!
//! # Feature flags
//!
//! - `elf`: Enables `zircon_object::util::elf_loader`.
//! - `hypervisor`: Enables `zircon_object::hypervisor` (`Guest` and `Vcpu`).

#![no_std]
#![deny(warnings)]
// The `impl_downcast!` macro from `downcast-rs` triggers this lint; we cannot fix upstream.
#![allow(clippy::multiple_bound_locations)]
// #![deny(missing_docs)] // effectively unenforced

extern crate alloc;

#[macro_use]
extern crate log;

#[cfg(test)]
#[macro_use]
extern crate std;

pub mod debuglog;
pub mod dev;
mod error;
#[cfg(feature = "hypervisor")]
pub mod hypervisor;
pub mod ipc;
pub mod object;
pub mod signal;
pub mod task;
pub mod util;
pub mod vm;

pub use self::error::*;
