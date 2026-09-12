//! Zircon socket wrapper.

use crate::handle::RawHandle;
use crate::{impl_handle_based, Handle, HandleBased, Status};
use zircon_abi::syscall;

/// A Zircon socket endpoint.
#[derive(Debug)]
pub struct Socket(Handle);
impl_handle_based!(Socket);

impl Socket {
    /// Create a new socket pair.
    pub fn create(options: u32) -> Result<(Socket, Socket), Status> {
        let mut h0: RawHandle = 0;
        let mut h1: RawHandle = 0;
        Status::ok(unsafe { syscall::zx_socket_create(options, &mut h0, &mut h1) })?;
        unsafe { Ok((Socket(Handle::from_raw(h0)), Socket(Handle::from_raw(h1)))) }
    }

    /// Write data to the socket.
    ///
    /// Returns the number of bytes actually written.
    pub fn write(&self, data: &[u8]) -> Result<usize, Status> {
        let mut actual: usize = 0;
        Status::ok(unsafe {
            syscall::zx_socket_write(self.raw_handle(), 0, data.as_ptr(), data.len(), &mut actual)
        })?;
        Ok(actual)
    }

    /// Read data from the socket.
    ///
    /// Returns the number of bytes actually read.
    pub fn read(&self, buf: &mut [u8]) -> Result<usize, Status> {
        let mut actual: usize = 0;
        Status::ok(unsafe {
            syscall::zx_socket_read(
                self.raw_handle(),
                0,
                buf.as_mut_ptr(),
                buf.len(),
                &mut actual,
            )
        })?;
        Ok(actual)
    }
}
