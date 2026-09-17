//! Kernel-internal device drivers for zCore.
//!
//! Contains only boot-essential drivers: interrupt controllers, UART,
//! PCI bus, VirtIO (behind feature gate), and the driver trait
//! infrastructure.
//!
//! Non-essential drivers (network, display, input, mock) live in the
//! separate `drivers` crate under `petal/` for future reference.

#![cfg_attr(not(feature = "libos"), no_std)]
#![deny(warnings)]

extern crate alloc;

#[macro_use]
extern crate log;

use alloc::sync::Arc;
use core::fmt;

// --- Driver modules ---
// Each optional driver is gated by its own feature flag.
// Arch-specific drivers (APIC, GIC, PLIC, PL011, Uart16550Pmio) are
// gated by target_arch since they can't compile on other architectures.

/// PCI bus enumeration.
pub mod bus;
/// Interrupt controller drivers.
pub mod irq;
/// Keyboard input drivers.
pub mod keyboard;
/// UART serial port drivers.
pub mod uart;
/// VirtIO device drivers (block, console, GPU, input).
#[cfg(any(feature = "virtio", doc))]
pub mod virtio;

// --- Infrastructure modules (always compiled) ---
pub mod builder;
pub mod io;
pub mod prelude;
pub mod scheme;
pub mod utils;

/// The error type for external device.
#[derive(Debug)]
pub enum DeviceError {
    /// The buffer is too small.
    BufferTooSmall,
    /// The device is not ready.
    NotReady,
    /// Invalid parameter.
    InvalidParam,
    /// Failed to alloc DMA memory.
    DmaError,
    /// I/O Error
    IoError,
    /// A resource with the specified identifier already exists.
    AlreadyExists,
    /// No resource to allocate.
    NoResources,
    /// The device driver is not implemented, supported, or enabled.
    NotSupported,
}

/// A type alias for the result of a device operation.
pub type DeviceResult<T = ()> = core::result::Result<T, DeviceError>;

/// Static shell of shared dynamic device [`Scheme`](crate::scheme::Scheme) types.
#[derive(Clone)]
pub enum Device {
    /// Block device
    Block(Arc<dyn scheme::BlockScheme>),
    /// Display device
    Display(Arc<dyn scheme::DisplayScheme>),
    /// Input device
    Input(Arc<dyn scheme::InputScheme>),
    /// Interrupt request and handle
    Irq(Arc<dyn scheme::IrqScheme>),
    /// Uart port
    Uart(Arc<dyn scheme::UartScheme>),
}

impl Device {
    /// Get a general [`Scheme`](scheme::Scheme) from the device.
    pub fn inner(&self) -> Arc<dyn scheme::Scheme> {
        match self {
            Self::Block(d) => d.clone().upcast(),
            Self::Display(d) => d.clone().upcast(),
            Self::Input(d) => d.clone().upcast(),
            Self::Irq(d) => d.clone().upcast(),
            Self::Uart(d) => d.clone().upcast(),
        }
    }
}

impl fmt::Debug for Device {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::Block(d) => write!(f, "BlockDevice({:?})", d.name()),
            Self::Display(d) => write!(f, "DisplayDevice({:?})", d.name()),
            Self::Input(d) => write!(f, "InputDevice({:?})", d.name()),
            Self::Irq(d) => write!(f, "IrqDevice({:?})", d.name()),
            Self::Uart(d) => write!(f, "UartDevice({:?})", d.name()),
        }
    }
}

type PhysAddr = usize;
type VirtAddr = usize;
