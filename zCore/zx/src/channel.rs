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
    ///
    /// Handles are consumed by the kernel on success or failure.
    /// The `Handle` objects are taken by value and will NOT be
    /// closed on drop (ownership transfers to the kernel).
    pub fn write(&self, bytes: &[u8], handles: &mut [Handle]) -> Result<(), Status> {
        // Extract raw values and forget the Handle wrappers so Drop
        // doesn't close them -- the kernel consumes them.
        let raw_handles: &[RawHandle] =
            // Safety: Handle is repr(transparent) over RawHandle via the
            // inner Handle(RawHandle) newtype, so &[Handle] and &[RawHandle]
            // have the same layout.  We forget ownership below.
            unsafe { core::slice::from_raw_parts(handles.as_ptr() as *const RawHandle, handles.len()) };
        let result = Status::ok(unsafe {
            syscall::zx_channel_write(
                self.raw_handle(),
                0,
                bytes.as_ptr(),
                bytes.len() as u32,
                raw_handles.as_ptr(),
                raw_handles.len() as u32,
            )
        });
        // The kernel consumed (or rejected) all handles. Zero out the
        // slots so the Handle Drops become no-ops.
        for h in handles.iter_mut() {
            // Safety: zeroing the raw value prevents double-close.
            unsafe { core::ptr::write(h as *mut Handle, Handle::from_raw(0)) };
        }
        result
    }

    /// Write a message (bytes only, no handles) to the channel.
    pub fn write_bytes(&self, bytes: &[u8]) -> Result<(), Status> {
        Status::ok(unsafe {
            syscall::zx_channel_write(
                self.raw_handle(),
                0,
                bytes.as_ptr(),
                bytes.len() as u32,
                core::ptr::null(),
                0,
            )
        })
    }

    /// Read a message from the channel.
    ///
    /// Returns `(actual_bytes, actual_handles)` on success.
    /// Received handles are returned as owned `Handle` values.
    pub fn read(&self, bytes: &mut [u8], handles: &mut [Handle]) -> Result<(usize, usize), Status> {
        let mut actual_bytes: u32 = 0;
        let mut actual_handles: u32 = 0;
        // Read raw handle values into the Handle slots.
        // Safety: Handle is a newtype over RawHandle with the same layout.
        let raw_handles: &mut [RawHandle] = unsafe {
            core::slice::from_raw_parts_mut(handles.as_mut_ptr() as *mut RawHandle, handles.len())
        };
        Status::ok(unsafe {
            syscall::zx_channel_read(
                self.raw_handle(),
                0,
                bytes.as_mut_ptr(),
                raw_handles.as_mut_ptr(),
                bytes.len() as u32,
                raw_handles.len() as u32,
                &mut actual_bytes,
                &mut actual_handles,
            )
        })?;
        Ok((actual_bytes as usize, actual_handles as usize))
    }

    /// Read a message (bytes only, no handles) from the channel.
    ///
    /// Returns the number of bytes read.
    pub fn read_bytes(&self, bytes: &mut [u8]) -> Result<usize, Status> {
        let mut actual_bytes: u32 = 0;
        let mut actual_handles: u32 = 0;
        Status::ok(unsafe {
            syscall::zx_channel_read(
                self.raw_handle(),
                0,
                bytes.as_mut_ptr(),
                core::ptr::null_mut(),
                bytes.len() as u32,
                0,
                &mut actual_bytes,
                &mut actual_handles,
            )
        })?;
        Ok(actual_bytes as usize)
    }
}
