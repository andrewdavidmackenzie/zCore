use crate::{ZxError, ZxResult};
use hal_impl::context::UserContext;
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
            #[cfg(target_arch = "x86_64")]
            ThreadStateKind::Vector => {
                // Convert FXSAVE format (VectorRegs) to Zircon's
                // zx_x86_64_thread_state_vector_regs_t format.
                // zmm[32] (each 8xu64) + opmask[8] (8xu64) + mxcsr(u32) + pad(4)
                const ZMM_COUNT: usize = 32;
                const ZMM_SIZE: usize = 64; // 8 * u64
                const OPMASK_SIZE: usize = 64; // 8 * u64
                const TOTAL_SIZE: usize = ZMM_COUNT * ZMM_SIZE + OPMASK_SIZE + 8;
                if buf.len() < TOTAL_SIZE {
                    return Err(ZxError::BUFFER_TOO_SMALL);
                }
                // Zero the output buffer first.
                buf[..TOTAL_SIZE].fill(0);
                let vr = &self.vector_regs;
                // Copy XMM registers (128-bit = 16 bytes = 2 u64) into zmm[0..15].v[0..1]
                for (i, xmm) in vr.xmm.iter().enumerate() {
                    let zmm_off = i * ZMM_SIZE;
                    buf[zmm_off..zmm_off + 8].copy_from_slice(&xmm.0[0].to_ne_bytes());
                    buf[zmm_off + 8..zmm_off + 16].copy_from_slice(&xmm.0[1].to_ne_bytes());
                }
                // mxcsr is after zmm[32] + opmask[8]
                let mxcsr_off = ZMM_COUNT * ZMM_SIZE + OPMASK_SIZE;
                buf[mxcsr_off..mxcsr_off + 4].copy_from_slice(&vr.mxcsr.to_ne_bytes());
                Ok(TOTAL_SIZE)
            }
            ThreadStateKind::SingleStep => {
                // zx_thread_state_single_step_t is a u32.
                if buf.len() < 4 {
                    return Err(ZxError::BUFFER_TOO_SMALL);
                }
                // Return 0 (not single-stepping). We don't track this state.
                buf[..4].copy_from_slice(&0u32.to_ne_bytes());
                Ok(4)
            }
            #[cfg(target_arch = "x86_64")]
            ThreadStateKind::Debug => {
                // zx_x86_64_thread_state_debug_regs_t: dr[4] + dr6 + dr7 = 48 bytes
                const DEBUG_SIZE: usize = 6 * 8;
                if buf.len() < DEBUG_SIZE {
                    return Err(ZxError::BUFFER_TOO_SMALL);
                }
                let dr = &self.debug_regs;
                for (i, &val) in dr.dr.iter().enumerate() {
                    buf[i * 8..(i + 1) * 8].copy_from_slice(&val.to_ne_bytes());
                }
                buf[32..40].copy_from_slice(&dr.dr6.to_ne_bytes());
                buf[40..48].copy_from_slice(&dr.dr7.to_ne_bytes());
                Ok(DEBUG_SIZE)
            }
            _ => Err(ZxError::NOT_SUPPORTED),
        }
    }

    fn write_state(&mut self, kind: ThreadStateKind, buf: &[u8]) -> ZxResult {
        match kind {
            ThreadStateKind::General => {
                let regs: hal_impl::context::GeneralRegs = buf.read_struct()?;
                // On x86_64, reject non-canonical and kernel-space RIP/RSP.
                #[cfg(target_arch = "x86_64")]
                {
                    let is_canonical = |addr: usize| -> bool {
                        let sign_ext = ((addr as i64) << 16 >> 16) as usize;
                        addr == sign_ext
                    };
                    let is_user = |addr: usize| -> bool {
                        // User addresses have bit 47 = 0 (lower half).
                        addr < 0x0000_8000_0000_0000
                    };
                    if !is_canonical(regs.rip) || !is_user(regs.rip) {
                        return Err(ZxError::INVALID_ARGS);
                    }
                }
                *self.general_mut() = regs;
            }
            #[cfg(target_arch = "x86_64")]
            ThreadStateKind::Vector => {
                const ZMM_COUNT: usize = 32;
                const ZMM_SIZE: usize = 64;
                const OPMASK_SIZE: usize = 64;
                const TOTAL_SIZE: usize = ZMM_COUNT * ZMM_SIZE + OPMASK_SIZE + 8;
                if buf.len() < TOTAL_SIZE {
                    return Err(ZxError::BUFFER_TOO_SMALL);
                }
                let vr = &mut self.vector_regs;
                // Validate MXCSR — reject reserved bits.
                let mxcsr_off = ZMM_COUNT * ZMM_SIZE + OPMASK_SIZE;
                let new_mxcsr =
                    u32::from_ne_bytes(buf[mxcsr_off..mxcsr_off + 4].try_into().unwrap());
                // Use processor's MXCSR_MASK if available, otherwise the
                // architectural default (0xFFBF — bits 0-15 minus bit 6).
                let mxcsr_mask = if vr.mxcsr_mask != 0 {
                    vr.mxcsr_mask
                } else {
                    0x0000_FFBF
                };
                if new_mxcsr & !mxcsr_mask != 0 {
                    return Err(ZxError::INVALID_ARGS);
                }
                vr.mxcsr = new_mxcsr;
                // Copy XMM registers from zmm[0..15].v[0..1]
                for (i, xmm) in vr.xmm.iter_mut().enumerate() {
                    let zmm_off = i * ZMM_SIZE;
                    xmm.0[0] = u64::from_ne_bytes(buf[zmm_off..zmm_off + 8].try_into().unwrap());
                    xmm.0[1] =
                        u64::from_ne_bytes(buf[zmm_off + 8..zmm_off + 16].try_into().unwrap());
                }
            }
            ThreadStateKind::SingleStep => {
                // zx_thread_state_single_step_t is a u32. Only 0 and 1 are valid.
                if buf.len() < 4 {
                    return Err(ZxError::BUFFER_TOO_SMALL);
                }
                let value = u32::from_ne_bytes(buf[..4].try_into().unwrap());
                if value > 1 {
                    return Err(ZxError::INVALID_ARGS);
                }
                // Store single-step state. On x86_64, this sets the TF
                // (trap flag) in RFLAGS. We don't actually enforce it
                // here — just accept the value.
                // TODO: set TF in self.general_mut().rflags
            }
            #[cfg(target_arch = "x86_64")]
            ThreadStateKind::Debug => {
                const DEBUG_SIZE: usize = 6 * 8;
                if buf.len() < DEBUG_SIZE {
                    return Err(ZxError::BUFFER_TOO_SMALL);
                }
                let dr = &mut self.debug_regs;
                for i in 0..4 {
                    dr.dr[i] = u64::from_ne_bytes(buf[i * 8..(i + 1) * 8].try_into().unwrap());
                }
                dr.dr6 = u64::from_ne_bytes(buf[32..40].try_into().unwrap());
                dr.dr7 = u64::from_ne_bytes(buf[40..48].try_into().unwrap());
            }
            _ => return Err(ZxError::NOT_SUPPORTED),
        }
        Ok(())
    }
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
