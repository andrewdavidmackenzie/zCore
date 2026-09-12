// Wraps peripheral address space access, including memory-mapped I/O (MMIO) and port-mapped I/O (PMIO).
//
// To learn about these two methods of accessing peripherals, see [Wikipedia](https://en.wikipedia.org/wiki/Memory-mapped_I/O).
//! Peripheral address space access, including memory-mapped IO and port-mapped IO.
//!
//! About these two methods of performing I/O, see [wikipedia](https://en.wikipedia.org/wiki/Memory-mapped_I/O).

use core::ops::{BitAnd, BitOr, Not};

mod mmio;
#[cfg(target_arch = "x86_64")]
mod pmio;

pub use mmio::Mmio;
#[cfg(target_arch = "x86_64")]
pub use pmio::Pmio;

/// Interface for accessing device address space (MMIO or PMIO).
pub trait Io {
    // The type of the accessible object.
    /// The type of object to access.
    type Value: Copy
        + BitAnd<Output = Self::Value>
        + BitOr<Output = Self::Value>
        + Not<Output = Self::Value>;

    // Reads a value from the peripheral.
    /// Reads value from device.
    fn read(&self) -> Self::Value;

    // Writes a value to the peripheral.
    /// Writes `value` to device.
    fn write(&mut self, value: Self::Value);
}

// A read-only unit in the peripheral address space.
/// A readonly unit in device address space.
#[repr(transparent)]
pub struct ReadOnly<I>(I);

impl<I> ReadOnly<I> {
    // Constructs a read-only unit in the peripheral address space.
    /// Constructs a readonly unit in device address space.
    pub const fn new(inner: I) -> Self {
        Self(inner)
    }
}

impl<I: Io> ReadOnly<I> {
    // Reads a value from the peripheral.
    /// Reads value from device.
    #[inline(always)]
    pub fn read(&self) -> I::Value {
        self.0.read()
    }
}

// A write-only unit in the peripheral address space.
/// A write-only unit in device address space.
#[repr(transparent)]
pub struct WriteOnly<I>(I);

impl<I> WriteOnly<I> {
    // Constructs a write-only unit in the peripheral address space.
    /// Constructs a write-only unit in device address space.
    pub const fn new(inner: I) -> Self {
        Self(inner)
    }
}

impl<I: Io> WriteOnly<I> {
    // Writes a value to the peripheral.
    /// Writes `value` to device.
    #[inline(always)]
    pub fn write(&mut self, value: I::Value) {
        self.0.write(value);
    }
}
