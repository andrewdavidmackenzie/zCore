//! Hardware Abstraction Layer -- trait definitions and common types.
//!
//! This crate defines the interface contract between the kernel and
//! platform-specific implementations. It contains:
//!
//! - Common types (`PhysAddr`, `VirtAddr`, `PAGE_SIZE`)
//! - Base driver trait (`Scheme`, `SchemeUpcast`)
//! - Device error types (`DeviceError`, `DeviceResult`)
//!
//! No architecture-specific code lives here. Platform implementations
//! are in the `kernel-hal` crate (future: `hal-impl`).

#![no_std]
#![deny(warnings)]

extern crate alloc;

pub mod addr;
pub mod defs;
pub mod device;
pub mod scheme;

// Re-export commonly used types at the crate root.
pub use addr::{DevVAddr, PhysAddr, VirtAddr};
pub use defs::PAGE_SIZE;
pub use device::{DeviceError, DeviceResult};
pub use scheme::{Scheme, SchemeUpcast};
