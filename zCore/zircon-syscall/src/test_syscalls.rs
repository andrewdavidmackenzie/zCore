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
    /// anything.
    ///
    /// If the handle is created but the copyout fails (e.g. null
    /// pointer), the handle leaks and a
    /// `ZX_EXCP_POLICY_CODE_HANDLE_LEAK` exception is delivered.
    pub async fn sys_syscall_test_handle_create(
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
            Err(_e) => {
                // Handle leak: the handle was created but couldn't be
                // delivered to userspace.  Deliver a policy exception
                // (ExceptionType::PolicyError with synth_code=HANDLE_LEAK)
                // so the test's exception handler can observe it.
                const ZX_EXCP_POLICY_CODE_HANDLE_LEAK: u32 = 20;
                self.thread
                    .handle_exception_policy(ZX_EXCP_POLICY_CODE_HANDLE_LEAK, 0)
                    .await;
                ZxError::INVALID_ARGS as isize
            }
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
