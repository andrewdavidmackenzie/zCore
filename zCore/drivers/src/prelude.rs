//! Re-export most commonly used driver types.

pub use crate::scheme::{
    CapabilityType, ColorFormat, DisplayInfo, FrameBuffer, InputCapability, InputEvent,
    InputEventType, IrqHandler, IrqPolarity, IrqTriggerMode, Rectangle, RgbColor,
};
pub use crate::{Device, DeviceError, DeviceResult};
