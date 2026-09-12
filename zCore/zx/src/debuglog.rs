//! Zircon debug log wrapper.

use crate::handle::RawHandle;
use crate::{impl_handle_based, Handle, HandleBased, Status};
use zircon_abi::syscall;

/// A Zircon debug log object.
#[derive(Debug)]
pub struct DebugLog(Handle);
impl_handle_based!(DebugLog);

impl DebugLog {
    /// Create a debug log, optionally readable.
    pub fn create(resource: &Handle, options: u32) -> Result<DebugLog, Status> {
        let mut handle: RawHandle = 0;
        Status::ok(unsafe { syscall::zx_debuglog_create(resource.raw(), options, &mut handle) })?;
        unsafe { Ok(DebugLog(Handle::from_raw(handle))) }
    }

    /// Write a message to the debug log.
    pub fn write(&self, data: &[u8]) -> Result<(), Status> {
        Status::ok(unsafe {
            syscall::zx_debuglog_write(self.raw_handle(), 0, data.as_ptr(), data.len())
        })
    }
}

/// Write a string to the kernel debug serial port (no handle needed).
pub fn debug_write(msg: &[u8]) {
    syscall::debug_write(msg);
}
