//! Console input and output.

use crate::drivers;
use core::fmt::{Arguments, Result, Write};
use lock::Mutex;

struct SerialWriter;

static SERIAL_WRITER: Mutex<SerialWriter> = Mutex::new(SerialWriter);

impl Write for SerialWriter {
    fn write_str(&mut self, s: &str) -> Result {
        if let Some(uart) = drivers::all_uart().first() {
            uart.write_str(s).unwrap();
        } else {
            crate::hal_fn::console::console_write_early(s);
        }
        Ok(())
    }
}

struct DebugWriter;

static DEBUG_WRITER: Mutex<DebugWriter> = Mutex::new(DebugWriter);

impl Write for DebugWriter {
    fn write_str(&mut self, s: &str) -> Result {
        crate::hal_fn::console::console_write_early(s);
        Ok(())
    }
}

// graphic console removed (see #237)

/// Writes a string slice into the serial.
pub fn serial_write_str(s: &str) {
    SERIAL_WRITER.lock().write_str(s).unwrap();
}

/// Writes formatted data into the serial.
pub fn serial_write_fmt(fmt: Arguments) {
    SERIAL_WRITER.lock().write_fmt(fmt).unwrap();
}

/// Writes a string slice into the serial through sbi call.
pub fn debug_write_str(s: &str) {
    DEBUG_WRITER.lock().write_str(s).unwrap();
}

/// Writes formatted data into the serial through sbi call..
pub fn debug_write_fmt(fmt: Arguments) {
    DEBUG_WRITER.lock().write_fmt(fmt).unwrap();
}

/// Writes a string slice into the graphic console (no-op, graphic removed).
#[allow(unused_variables)]
pub fn graphic_console_write_str(s: &str) {}

/// Writes formatted data into the graphic console (no-op, graphic removed).
#[allow(unused_variables)]
pub fn graphic_console_write_fmt(fmt: Arguments) {}

/// Writes a string slice into the serial, and the graphic console if it exists.
pub fn console_write_str(s: &str) {
    serial_write_str(s);
    graphic_console_write_str(s);
}

/// Writes formatted data into the serial, and the graphic console if it exists.
pub fn console_write_fmt(fmt: Arguments) {
    serial_write_fmt(fmt);
    graphic_console_write_fmt(fmt);
}

/// Read buffer data from console (serial).
pub async fn console_read(buf: &mut [u8]) -> usize {
    super::future::SerialReadFuture::new(buf).await
}

/// The POSIX `winsize` structure.
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct ConsoleWinSize {
    pub ws_row: u16,
    pub ws_col: u16,
    pub ws_xpixel: u16,
    pub ws_ypixel: u16,
}

/// Returns the size information of the console, see [`ConsoleWinSize`].
pub fn console_win_size() -> ConsoleWinSize {
    ConsoleWinSize::default()
}
