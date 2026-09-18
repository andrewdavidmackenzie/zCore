//! Driver trait definitions.
//!
//! Traits are defined in the `hal` crate and re-exported here.
//! The `impl_event_scheme!` macro and `EventListener` remain here
//! because they depend on the `lock` crate.

#[macro_use]
pub(super) mod event;
pub(super) use impl_event_scheme;

// Re-export all traits and types from the hal crate.
pub use hal::scheme::block::BlockScheme;
pub use hal::scheme::display::{
    self, ColorFormat, DisplayInfo, DisplayScheme, FrameBuffer, Rectangle, RgbColor,
};
pub use hal::scheme::event::{EventHandler, EventScheme};
pub use hal::scheme::input::{
    self, CapabilityType, InputCapability, InputEvent, InputEventType, InputScheme,
};
pub use hal::scheme::irq::{self, IrqHandler, IrqPolarity, IrqScheme, IrqTriggerMode};
pub use hal::scheme::uart::UartScheme;
pub use hal::scheme::{Scheme, SchemeUpcast};
