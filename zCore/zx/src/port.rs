//! Zircon port wrapper.

use crate::handle::RawHandle;
use crate::{impl_handle_based, Handle, HandleBased, Status};
use zircon_abi::syscall;

/// A Zircon port object for receiving asynchronous notifications.
#[derive(Debug)]
pub struct Port(Handle);
impl_handle_based!(Port);

impl Port {
    /// Create a new port.
    pub fn create() -> Result<Port, Status> {
        let mut handle: RawHandle = 0;
        Status::ok(unsafe { syscall::zx_port_create(0, &mut handle) })?;
        unsafe { Ok(Port(Handle::from_raw(handle))) }
    }

    /// Wait for a packet on the port.
    ///
    /// Blocks until a packet is available or the deadline expires.
    /// Returns the raw packet data (48 bytes).
    pub fn wait(&self, deadline: i64) -> Result<[u8; 48], Status> {
        let mut packet = [0u8; 48];
        Status::ok(unsafe {
            syscall::zx_port_wait(self.raw_handle(), deadline, packet.as_mut_ptr())
        })?;
        Ok(packet)
    }

    /// Queue a user packet to the port.
    pub fn queue(&self, packet: &[u8; 48]) -> Result<(), Status> {
        Status::ok(unsafe { syscall::zx_port_queue(self.raw_handle(), packet.as_ptr()) })
    }
}
