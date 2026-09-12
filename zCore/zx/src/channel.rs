//! Zircon channel wrapper.

use crate::handle::RawHandle;
use crate::{impl_handle_based, Handle, HandleBased, Status};
use zircon_abi::syscall;

/// A Zircon channel endpoint.
///
/// Channels are the primary IPC mechanism in Zircon. They transfer
/// messages consisting of bytes and handles between two endpoints.
/// Dropping a `Channel` closes the endpoint.
#[derive(Debug)]
pub struct Channel(Handle);
impl_handle_based!(Channel);

impl Channel {
    /// Create a new channel pair.
    pub fn create() -> Result<(Channel, Channel), Status> {
        let mut h0: RawHandle = 0;
        let mut h1: RawHandle = 0;
        Status::ok(unsafe { syscall::zx_channel_create(0, &mut h0, &mut h1) })?;
        unsafe { Ok((Channel(Handle::from_raw(h0)), Channel(Handle::from_raw(h1)))) }
    }

    /// Write a message (bytes and handles) to the channel.
    pub fn write(&self, bytes: &[u8], handles: &[RawHandle]) -> Result<(), Status> {
        Status::ok(unsafe {
            syscall::zx_channel_write(
                self.raw_handle(),
                0,
                bytes.as_ptr(),
                bytes.len() as u32,
                handles.as_ptr(),
                handles.len() as u32,
            )
        })
    }

    /// Read a message from the channel.
    ///
    /// Returns `(actual_bytes, actual_handles)` on success.
    pub fn read(
        &self,
        bytes: &mut [u8],
        handles: &mut [RawHandle],
    ) -> Result<(usize, usize), Status> {
        let mut actual_bytes: u32 = 0;
        let mut actual_handles: u32 = 0;
        Status::ok(unsafe {
            syscall::zx_channel_read(
                self.raw_handle(),
                0,
                bytes.as_mut_ptr(),
                handles.as_mut_ptr(),
                bytes.len() as u32,
                handles.len() as u32,
                &mut actual_bytes,
                &mut actual_handles,
            )
        })?;
        Ok((actual_bytes as usize, actual_handles as usize))
    }
}
