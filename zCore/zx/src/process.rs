//! Zircon process wrapper.

use crate::handle::RawHandle;
use crate::{impl_handle_based, Handle};
use zircon_abi::syscall;

/// A Zircon process object.
#[derive(Debug)]
pub struct Process(Handle);
impl_handle_based!(Process);

impl Process {
    /// Exit the current process with the given return code.
    pub fn exit(retcode: i64) -> ! {
        syscall::process_exit(retcode);
    }
}

/// Get the raw handle for the current process (handle index 0 from bootstrap).
///
/// Note: in petal programs the process self-handle is typically obtained
/// from the bootstrap channel, not from this function.
pub fn process_self() -> RawHandle {
    // In Zircon, ZX_HANDLE_INVALID (0) is used as a sentinel for "self"
    // in some syscalls. There is no dedicated process-self handle accessor.
    0
}
