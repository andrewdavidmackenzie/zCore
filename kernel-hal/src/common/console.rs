//! Console input and output.
//!
//! Kernel console output goes through `console_write_early`, which each
//! platform implements in the HAL. The implementation decides the physical
//! output: UART on aarch64/riscv, framebuffer on x86 laptops, etc.

use core::fmt::{Arguments, Result, Write};
use lock::Mutex;

struct ConsoleWriter;

static CONSOLE_WRITER: Mutex<ConsoleWriter> = Mutex::new(ConsoleWriter);

impl Write for ConsoleWriter {
    fn write_str(&mut self, s: &str) -> Result {
        crate::hal_fn::console::console_write_early(s);
        Ok(())
    }
}

/// Writes a string slice to the console.
pub fn console_write_str(s: &str) {
    CONSOLE_WRITER.lock().write_str(s).unwrap();
}

/// Writes formatted data to the console.
pub fn console_write_fmt(fmt: Arguments) {
    CONSOLE_WRITER.lock().write_fmt(fmt).unwrap();
}

/// Read buffer data from console.
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
