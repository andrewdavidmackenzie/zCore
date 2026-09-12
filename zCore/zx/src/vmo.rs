//! Zircon VMO (Virtual Memory Object) wrapper.

use crate::handle::RawHandle;
use crate::{impl_handle_based, Handle, HandleBased, Status};
use zircon_abi::syscall;

/// A Zircon Virtual Memory Object.
///
/// VMOs represent contiguous regions of virtual memory. They can be
/// read, written, mapped into address spaces, and shared between
/// processes.
#[derive(Debug)]
pub struct Vmo(Handle);
impl_handle_based!(Vmo);

impl Vmo {
    /// Create a new VMO of the given size (rounded up to page size).
    pub fn create(size: u64) -> Result<Vmo, Status> {
        let mut handle: RawHandle = 0;
        Status::ok(unsafe { syscall::zx_vmo_create(size, 0, &mut handle) })?;
        unsafe { Ok(Vmo(Handle::from_raw(handle))) }
    }

    /// Read bytes from the VMO at the given offset.
    pub fn read(&self, buf: &mut [u8], offset: u64) -> Result<(), Status> {
        Status::ok(unsafe {
            syscall::zx_vmo_read(self.raw_handle(), buf.as_mut_ptr(), offset, buf.len())
        })
    }

    /// Write bytes to the VMO at the given offset.
    pub fn write(&self, data: &[u8], offset: u64) -> Result<(), Status> {
        Status::ok(unsafe {
            syscall::zx_vmo_write(self.raw_handle(), data.as_ptr(), offset, data.len())
        })
    }

    /// Get the size of the VMO in bytes.
    pub fn get_size(&self) -> Result<usize, Status> {
        let mut size: usize = 0;
        Status::ok(unsafe { syscall::zx_vmo_get_size(self.raw_handle(), &mut size) })?;
        Ok(size)
    }

    /// Set the size of the VMO.
    pub fn set_size(&self, size: u64) -> Result<(), Status> {
        Status::ok(unsafe { syscall::zx_vmo_set_size(self.raw_handle(), size) })
    }
}
