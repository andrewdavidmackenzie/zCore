//! PS/2 keyboard driver via the i8042 controller.
//!
//! Reads scancodes from I/O port 0x60 on IRQ 1, decodes them using
//! the `pc-keyboard` crate (Scancode Set 1, US 104-key layout), and
//! pushes decoded ASCII bytes via a caller-provided callback.
//!
//! The callback is typically `console_input_push` from the HAL,
//! which feeds the shared console input buffer.

use crate::scheme::Scheme;
use alloc::boxed::Box;
use lock::Mutex;
use pc_keyboard::{layouts, DecodedKey, HandleControl, ScancodeSet1};

/// I/O port for the i8042 data register.
const I8042_DATA_PORT: u16 = 0x60;
/// I/O port for the i8042 status register.
const I8042_STATUS_PORT: u16 = 0x64;
/// Status bit: output buffer full (data available to read).
const STATUS_OUTPUT_FULL: u8 = 0x01;

/// Callback type for pushing decoded bytes to the console.
type InputCallback = dyn Fn(u8) + Send + Sync;

/// PS/2 keyboard driver.
///
/// Decodes i8042 scancodes and pushes ASCII bytes to the console
/// input via a callback provided at construction time.
pub struct Ps2Keyboard {
    inner: Mutex<pc_keyboard::PS2Keyboard<layouts::Us104Key, ScancodeSet1>>,
    on_input: Box<InputCallback>,
}

impl Ps2Keyboard {
    /// Create a new PS/2 keyboard driver.
    ///
    /// `on_input` is called for each decoded ASCII byte (from the IRQ handler).
    /// Typically this is `console_input_push`.
    ///
    /// Flushes any stale data from the i8042 output buffer.
    pub fn new(on_input: impl Fn(u8) + Send + Sync + 'static) -> Self {
        // Flush any stale bytes in the i8042 output buffer.
        unsafe {
            while x86_64::instructions::port::Port::<u8>::new(I8042_STATUS_PORT).read()
                & STATUS_OUTPUT_FULL
                != 0
            {
                let _ = x86_64::instructions::port::Port::<u8>::new(I8042_DATA_PORT).read();
            }
        }

        Self {
            inner: Mutex::new(pc_keyboard::PS2Keyboard::new(
                ScancodeSet1::new(),
                layouts::Us104Key,
                HandleControl::MapLettersToUnicode,
            )),
            on_input: Box::new(on_input),
        }
    }
}

impl Scheme for Ps2Keyboard {
    fn name(&self) -> &str {
        "ps2-keyboard"
    }

    fn handle_irq(&self, _irq_num: usize) {
        let scancode =
            unsafe { x86_64::instructions::port::Port::<u8>::new(I8042_DATA_PORT).read() };

        let mut kbd = self.inner.lock();
        if let Ok(Some(event)) = kbd.add_byte(scancode) {
            if let Some(key) = kbd.process_keyevent(event) {
                match key {
                    DecodedKey::Unicode(c) => {
                        if c.is_ascii() {
                            let b = c as u8;
                            let b = if b == b'\r' { b'\n' } else { b };
                            (self.on_input)(b);
                        }
                    }
                    DecodedKey::RawKey(_) => {
                        // Non-printable keys (arrows, function keys, etc.)
                        // are ignored for now.
                    }
                }
            }
        }
    }
}
