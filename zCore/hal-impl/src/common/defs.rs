//! Common type re-exports from the `hal` crate.

pub use hal::{CachePolicy, DeviceError, DeviceResult, MMUFlags, PAGE_SIZE};

pub use super::addr::{DevVAddr, PhysAddr, VirtAddr};
