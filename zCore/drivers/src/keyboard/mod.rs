//! Keyboard device drivers.

#[cfg(all(feature = "ps2-keyboard", target_arch = "x86_64"))]
mod ps2;

#[cfg(all(feature = "ps2-keyboard", target_arch = "x86_64"))]
pub use ps2::Ps2Keyboard;
