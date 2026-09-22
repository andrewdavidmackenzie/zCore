//! Console input and output.
//!
//! **Output** goes through `console_write_early`, which each platform
//! implements in the HAL. The implementation decides the physical
//! output: UART on aarch64/riscv, framebuffer on x86 laptops, etc.
//!
//! **Input** goes through a shared `ConsoleInput` ring buffer. Each
//! platform wires its input devices (UART, PS/2 keyboard, etc.) to
//! push bytes into this buffer. `console_read()` asynchronously reads
//! from it, regardless of the underlying input device.

use alloc::{collections::VecDeque, vec::Vec};
use core::fmt::{Arguments, Result, Write};
use core::task::Waker;
use lock::Mutex;

// --- Console output ---

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

// --- Console input ---

/// Maximum capacity of the console input buffer.
/// Prevents unbounded growth if input arrives faster than it's consumed.
const CONSOLE_INPUT_CAPACITY: usize = 4096;

/// Shared console input buffer.
///
/// Any input device (UART, PS/2 keyboard, USB HID, etc.) pushes bytes
/// here via `console_input_push()`. The `console_read()` function
/// asynchronously drains it.
struct ConsoleInput {
    buf: VecDeque<u8>,
    wakers: Vec<Waker>,
}

static CONSOLE_INPUT: Mutex<ConsoleInput> = Mutex::new(ConsoleInput {
    buf: VecDeque::new(),
    wakers: Vec::new(),
});

/// Push a byte into the console input buffer.
///
/// Called from IRQ handlers of input devices (UART, PS/2 keyboard, etc.).
/// Wakes any task waiting in `console_read()`. If the buffer is full,
/// the oldest byte is dropped.
pub fn console_input_push(byte: u8) {
    let mut input = CONSOLE_INPUT.lock();
    if input.buf.len() >= CONSOLE_INPUT_CAPACITY {
        input.buf.pop_front(); // drop oldest byte
    }
    input.buf.push_back(byte);
    for waker in input.wakers.drain(..) {
        waker.wake();
    }
}

/// Try to read bytes from the console input buffer. If no data is
/// available and a `waker` is provided, registers it for notification
/// when new input arrives. The check and registration are atomic
/// (under the same lock) to prevent a race where input arrives
/// between the empty check and waker registration.
///
/// Returns the number of bytes read (0 if empty).
pub fn console_input_poll(buf: &mut [u8], waker: Option<Waker>) -> usize {
    let mut input = CONSOLE_INPUT.lock();
    let mut n = 0;
    while n < buf.len() {
        if let Some(b) = input.buf.pop_front() {
            buf[n] = b;
            n += 1;
        } else {
            break;
        }
    }
    if n == 0 {
        if let Some(w) = waker {
            input.wakers.push(w);
        }
    }
    n
}

/// Read bytes from the console asynchronously.
///
/// Waits until at least one byte is available from any input device.
pub async fn console_read(buf: &mut [u8]) -> usize {
    super::future::ConsoleReadFuture::new(buf).await
}

// --- Console window size ---

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
