//! Driver trait definitions.
//!
//! The base [`Scheme`] trait must be implemented by all device drivers.
//! Specific device traits (`UartScheme`, `IrqScheme`, etc.) extend it.
//!
//! These traits define the interface contract between the kernel and
//! device drivers. Implementations live in the `drivers` crate.

use alloc::sync::Arc;

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
