//! Driver trait definitions.
//!
//! The base [`Scheme`] trait must be implemented by all device drivers.
//! Specific device traits ([`UartScheme`], [`IrqScheme`], etc.) extend it.
//!
//! These traits define the interface contract between the kernel and
//! device drivers. Implementations live in the `drivers` crate.

pub mod block;
pub mod display;
pub mod event;
pub mod input;
pub mod irq;
pub mod uart;

use alloc::sync::Arc;

pub use block::BlockScheme;
pub use display::DisplayScheme;
pub use event::EventScheme;
pub use input::InputScheme;
pub use irq::IrqScheme;
pub use uart::UartScheme;

/// Common trait for all device drivers.
///
/// Every device must provide a name and can optionally handle interrupts.
pub trait Scheme: SchemeUpcast + Send + Sync {
    /// Returns name of the driver.
    fn name(&self) -> &str;

    /// Handles an interrupt.
    fn handle_irq(&self, _irq_num: usize) {}
}

/// Used to convert a concrete type pointer to a general [`Scheme`] pointer.
pub trait SchemeUpcast {
    /// Performs the conversion.
    fn upcast<'a>(self: Arc<Self>) -> Arc<dyn Scheme + 'a>
    where
        Self: 'a;
}

impl<T: Scheme + Sized> SchemeUpcast for T {
    fn upcast<'a>(self: Arc<Self>) -> Arc<dyn Scheme + 'a>
    where
        Self: 'a,
    {
        self
    }
}
