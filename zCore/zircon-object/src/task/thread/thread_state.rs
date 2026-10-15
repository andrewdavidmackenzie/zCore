use crate::{ZxError, ZxResult};
use hal_impl::context::{GeneralRegs, UserContext};
use numeric_enum_macro::numeric_enum;

numeric_enum! {
    #[repr(u32)]
    /// Possible values for "kind" in zx_thread_read_state and zx_thread_write_state.
    #[allow(missing_docs)]
    #[derive(Debug, Copy, Clone)]
    pub enum ThreadStateKind {
        General = 0,
        FloatPoint = 1,
        Vector = 2,
        Debug = 4,
        SingleStep = 5,
    }
}

pub(super) trait ContextAccessState {
    fn read_state(&self, kind: ThreadStateKind, buf: &mut [u8]) -> ZxResult<usize>;
    fn write_state(&mut self, kind: ThreadStateKind, buf: &[u8]) -> ZxResult;
}

impl ContextAccessState for UserContext {
    fn read_state(&self, kind: ThreadStateKind, buf: &mut [u8]) -> ZxResult<usize> {
        match kind {
            ThreadStateKind::General => buf.write_struct(self.general()),
            _ => Err(ZxError::NOT_SUPPORTED),
        }
    }

    fn write_state(&mut self, kind: ThreadStateKind, buf: &[u8]) -> ZxResult {
        match kind {
            ThreadStateKind::General => {
                let regs: GeneralRegs = buf.read_struct()?;
                validate_general_regs(&regs)?;
                *self.general_mut() = regs;
            }
            _ => return Err(ZxError::NOT_SUPPORTED),
        }
        Ok(())
    }
}

/// Validate general register values before writing them to thread state.
///
/// On x86_64, rejects:
/// - Non-canonical RIP (bits 48..63 don't sign-extend bit 47)
/// - Kernel-space RIP (>= 0xFFFF_8000_0000_0000)
/// - Non-canonical fsbase or gsbase
/// - Kernel-space fsbase or gsbase
///
/// This matches Fuchsia's `arch_validate_general_regs()`.
fn validate_general_regs(regs: &GeneralRegs) -> ZxResult {
    #[cfg(target_arch = "x86_64")]
    {
        if !is_valid_user_address(regs.rip) {
            return Err(ZxError::INVALID_ARGS);
        }
        if !is_valid_user_address(regs.fsbase) {
            return Err(ZxError::INVALID_ARGS);
        }
        if !is_valid_user_address(regs.gsbase) {
            return Err(ZxError::INVALID_ARGS);
        }
    }
    #[cfg(not(target_arch = "x86_64"))]
    {
        let _ = regs;
    }
    Ok(())
}

/// Check whether `addr` is a valid user-space address on x86_64.
///
/// A valid user address is canonical (bits 48..63 sign-extend bit 47)
/// AND in the lower half of the address space (bit 63 == 0).
/// Address 0 is valid (it will fault at runtime, but it's not the
/// kernel's job to reject it in write_state).
#[cfg(target_arch = "x86_64")]
fn is_valid_user_address(addr: usize) -> bool {
    // The canonical check: sign-extend bit 47 across bits 48..63.
    // If bits 48..63 are all 0, it's canonical user-space (lower half).
    // If bits 48..63 are all 1, it's canonical kernel-space (upper half).
    // Anything else is non-canonical.
    let canonical = ((addr as isize) << 16) >> 16;
    if canonical as usize != addr {
        // Non-canonical address
        return false;
    }
    // Reject kernel-space addresses (upper canonical half: bit 63 set)
    if addr >= 0xFFFF_8000_0000_0000 {
        return false;
    }
    true
}

trait BufExt {
    fn read_struct<T>(&self) -> ZxResult<T>;
    fn write_struct<T: Copy>(&mut self, value: &T) -> ZxResult<usize>;
}

#[allow(unsafe_code)]
impl BufExt for [u8] {
    fn read_struct<T>(&self) -> ZxResult<T> {
        if self.len() < core::mem::size_of::<T>() {
            return Err(ZxError::BUFFER_TOO_SMALL);
        }
        Ok(unsafe { (self.as_ptr() as *const T).read() })
    }

    fn write_struct<T: Copy>(&mut self, value: &T) -> ZxResult<usize> {
        if self.len() < core::mem::size_of::<T>() {
            return Err(ZxError::BUFFER_TOO_SMALL);
        }
        unsafe {
            *(self.as_mut_ptr() as *mut T) = *value;
        }
        Ok(core::mem::size_of::<T>())
    }
}
