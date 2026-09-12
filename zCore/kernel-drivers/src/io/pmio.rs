// Port-mapped I/O (PMIO).
//! Port-mapped I/O.

use super::Io;
use core::{arch::asm, marker::PhantomData};

// Port-mapped I/O (PMIO).
/// Port-mapped I/O.
#[derive(Copy, Clone)]
pub struct Pmio<T> {
    port: u16,
    _phantom: PhantomData<T>,
}

impl<T> Pmio<T> {
    // Maps a given port for peripheral access.
    /// Maps a given port to assess device.
    pub const fn new(port: u16) -> Self {
        Self {
            port,
            _phantom: PhantomData,
        }
    }
}

// Byte-wise PMIO read/write.
/// Read/Write for byte PMIO.
impl Io for Pmio<u8> {
    type Value = u8;

    // Read.
    /// Read.
    #[inline(always)]
    fn read(&self) -> u8 {
        let value: u8;
        unsafe {
            asm!("in al, dx", out("al") value, in("dx") self.port, options(nomem, nostack, preserves_flags));
        }
        value
    }

    // Write.
    /// Write.
    #[inline(always)]
    fn write(&mut self, value: u8) {
        unsafe {
            asm!("out dx, al", in("al") value, in("dx") self.port, options(nomem, nostack, preserves_flags));
        }
    }
}

// Word-wise PMIO read/write.
/// Read/Write for word PMIO.
impl Io for Pmio<u16> {
    type Value = u16;

    // Read.
    /// Read.
    #[inline(always)]
    fn read(&self) -> u16 {
        let value: u16;
        unsafe {
            asm!("in ax, dx", out("ax") value, in("dx") self.port, options(nomem, nostack, preserves_flags));
        }
        value
    }

    // Write.
    /// Write.
    #[inline(always)]
    fn write(&mut self, value: u16) {
        unsafe {
            asm!("out dx, ax", in("ax") value, in("dx") self.port, options(nomem, nostack, preserves_flags));
        }
    }
}

// Double-word PMIO read/write.
/// Read/Write for double-word PMIO.
impl Io for Pmio<u32> {
    type Value = u32;

    // Read.
    /// Read.
    #[inline(always)]
    fn read(&self) -> u32 {
        let value: u32;
        unsafe {
            asm!("in eax, dx", out("eax") value, in("dx") self.port, options(nomem, nostack, preserves_flags));
        }
        value
    }

    // Write.
    /// Write.
    #[inline(always)]
    fn write(&mut self, value: u32) {
        unsafe {
            asm!("out dx, eax", in("eax") value, in("dx") self.port, options(nomem, nostack, preserves_flags));
        }
    }
}
