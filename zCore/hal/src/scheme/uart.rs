//! UART serial port driver trait.

use super::{event::EventScheme, Scheme};
use crate::DeviceResult;

/// Trait for UART serial port drivers.
pub trait UartScheme: Scheme + EventScheme<Event = ()> {
    /// Try to receive a byte without blocking.
    fn try_recv(&self) -> DeviceResult<Option<u8>>;

    /// Send a single byte.
    fn send(&self, ch: u8) -> DeviceResult;

    /// Send a string as a sequence of bytes.
    fn write_str(&self, s: &str) -> DeviceResult {
        for c in s.bytes() {
            self.send(c)?;
        }
        Ok(())
    }
}
