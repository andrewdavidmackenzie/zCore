//! Hardware Abstraction Layer -- trait definitions and common types.
//!
//! This crate defines the interface contract between the kernel and
//! platform-specific implementations. It contains:
//!
//! - Common types (`PhysAddr`, `VirtAddr`, `MMUFlags`, `PAGE_SIZE`, `CachePolicy`)
//! - Driver traits (`Scheme`, `UartScheme`, `IrqScheme`, `BlockScheme`,
//!   `DisplayScheme`, `InputScheme`, `EventScheme`)
//! - Page table trait (`GenericPageTable`)
//! - Kernel handler trait (`KernelHandler`)
//! - Trap/context types (`TrapReason`, `UserContextField`)
//! - Device error types (`DeviceError`, `DeviceResult`)
//!
//! No architecture-specific code lives here. Platform implementations
//! are in the `hal-impl` crate (Cargo package name: `hal-impl`).

#![no_std]
#![deny(warnings)]

extern crate alloc;

pub mod addr;
pub mod config;
pub mod context;
pub mod defs;
pub mod device;
pub mod kernel_handler;
pub mod scheme;
pub mod vm;

// Re-export commonly used types at the crate root.
pub use addr::{DevVAddr, PhysAddr, VirtAddr};
pub use config::KernelConfig;
pub use context::{TrapReason, UserContextField};
pub use defs::{CachePolicy, MMUFlags, PAGE_SIZE};
pub use device::{DeviceError, DeviceResult};
pub use kernel_handler::KernelHandler;
pub use scheme::{
    BlockScheme, DisplayScheme, EventScheme, InputScheme, IrqScheme, Scheme, SchemeUpcast,
    UartScheme,
};
pub use vm::{GenericPageTable, Page, PageSize, PagingError, PagingResult};
