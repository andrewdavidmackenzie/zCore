//! Mock devices, including display, input, uart and graphic.

pub mod display;
pub mod input;
pub mod uart;

#[cfg(any(feature = "graphic", doc))]
pub mod graphic;
