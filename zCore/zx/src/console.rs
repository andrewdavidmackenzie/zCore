//! Console I/O for petal programs.
//!
//! Provides simple serial console access via `debug_write` (output)
//! and `debug_read` (input). The `debug_read` syscall requires a
//! resource handle with root access.

use crate::handle::RawHandle;
use zircon_abi::syscall;

/// Write bytes to the serial console.
pub fn write(data: &[u8]) {
    syscall::debug_write(data);
}

/// Write a string to the serial console.
pub fn print(msg: &str) {
    syscall::debug_print(msg);
}

/// Read bytes from the serial console.
///
/// Requires a resource handle with root access (obtained from the
/// bootstrap channel). Returns the number of bytes read, or an error.
pub fn read(resource: RawHandle, buf: &mut [u8]) -> Result<usize, i32> {
    syscall::debug_read(resource, buf)
}
