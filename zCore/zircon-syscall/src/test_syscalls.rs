//! Zircon test-only syscalls.
//!
//! These are used by `SyscallGenerationTest` in core-tests to verify
//! the syscall ABI (argument passing, return values, handle copyout).

use super::*;
use zircon_object::signal::Event;

impl Syscall<'_> {
    /// Create a test handle (an event).
    ///
    /// If `retval` is `ZX_OK` (0), creates an event and writes the
    /// handle to `*out`.  Otherwise returns `retval` without creating
    /// anything.  Returns the raw isize result directly.
    pub fn sys_syscall_test_handle_create(
        &self,
        retval: i32,
        mut out: UserOutPtr<HandleValue>,
    ) -> isize {
        if retval != 0 {
            return retval as isize;
        }
        let event = Event::new();
        let proc = self.thread.proc();
        let handle = proc.add_handle(Handle::new(event, Rights::DEFAULT_EVENT));
        match out.write(handle) {
            Ok(()) => 0,
            Err(e) => e as isize,
        }
    }

    /// Test syscall: write the handle value to an output pointer.
    pub fn sys_syscall_test_rust_handle(
        &self,
        handle: HandleValue,
        mut out: UserOutPtr<u32>,
    ) -> ZxResult {
        out.write(handle)?;
        Ok(())
    }

    /// Test syscall: read an i32 from `in_ptr`, write it to `out_ptr`.
    pub fn sys_syscall_test_rust_inptr(
        &self,
        in_ptr: UserInPtr<i32>,
        mut out_ptr: UserOutPtr<i32>,
    ) -> ZxResult {
        if in_ptr.is_null() {
            return Err(ZxError::INVALID_ARGS);
        }
        let val = in_ptr.read()?;
        out_ptr.write(val)?;
        Ok(())
    }

    /// Test syscall: write a value to `out_ptr`.
    pub fn sys_syscall_test_rust_outptr(
        &self,
        value: i32,
        mut out_ptr: UserOutPtr<i32>,
    ) -> ZxResult {
        if out_ptr.is_null() {
            return Err(ZxError::INVALID_ARGS);
        }
        out_ptr.write(value)?;
        Ok(())
    }

    /// Test syscall: read from `in_out_ptr`, write value+1 back.
    pub fn sys_syscall_test_rust_inoutptr(&self, mut in_out_ptr: UserInOutPtr<i32>) -> ZxResult {
        if in_out_ptr.is_null() {
            return Err(ZxError::INVALID_ARGS);
        }
        let val = in_out_ptr.read()?;
        in_out_ptr.write(val + 1)?;
        Ok(())
    }
}
