//! Zircon timer wrapper.

use crate::handle::RawHandle;
use crate::{impl_handle_based, Handle, HandleBased, Status};
use zircon_abi::syscall;

/// A Zircon timer object.
#[derive(Debug)]
pub struct Timer(Handle);
impl_handle_based!(Timer);

impl Timer {
    /// Create a new timer.
    pub fn create() -> Result<Timer, Status> {
        let mut handle: RawHandle = 0;
        Status::ok(unsafe { syscall::zx_timer_create(0, 0, &mut handle) })?;
        unsafe { Ok(Timer(Handle::from_raw(handle))) }
    }

    /// Set the timer to fire at `deadline` with the given `slack`.
    pub fn set(&self, deadline: i64, slack: u64) -> Result<(), Status> {
        Status::ok(unsafe { syscall::zx_timer_set(self.raw_handle(), deadline, slack) })
    }

    /// Cancel a pending timer.
    pub fn cancel(&self) -> Result<(), Status> {
        Status::ok(unsafe { syscall::zx_timer_cancel(self.raw_handle()) })
    }
}
