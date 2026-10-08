//! Event handler and device tree.

mod event_listener;
mod id_allocator;
mod irq_manager;

pub mod devicetree;

pub(super) use id_allocator::IdAllocator;
pub(super) use irq_manager::IrqManager;

pub use event_listener::{EventHandler, EventListener};
