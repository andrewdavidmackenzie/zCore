//! Event handler, IRQ management, and device tree utilities.

mod event_listener;
pub use event_listener::{EventHandler, EventListener};

// IRQ manager and ID allocator are only needed by interrupt controller drivers.
#[cfg(any(feature = "apic", feature = "gic-400", feature = "riscv-plic"))]
mod id_allocator;
#[cfg(any(feature = "apic", feature = "gic-400", feature = "riscv-plic"))]
mod irq_manager;
#[cfg(any(feature = "apic", feature = "gic-400", feature = "riscv-plic"))]
pub(super) use id_allocator::IdAllocator;
#[cfg(any(feature = "apic", feature = "gic-400", feature = "riscv-plic"))]
pub(super) use irq_manager::IrqManager;

pub mod devicetree;
