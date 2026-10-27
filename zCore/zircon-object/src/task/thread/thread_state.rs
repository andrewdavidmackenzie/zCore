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
            #[cfg(target_arch = "x86_64")]
            ThreadStateKind::FloatPoint => {
                // zx_x86_64_thread_state_fp_regs_t layout:
                //   fcw(2) + fsw(2) + ftw(1) + reserved(1) + fop(2) = 8
                //   fip(8) + fdp(8) = 16
                //   padding(8) = 8  (align st[0] to 16 bytes at offset 32)
                //   st[8] each { low: u64, high: u64 } = 128
                //   Total = 160 bytes
                const FP_SIZE: usize = 160;
                if buf.len() < FP_SIZE {
                    return Err(ZxError::BUFFER_TOO_SMALL);
                }
                buf[..FP_SIZE].fill(0);
                let vr = &self.vector_regs;
                buf[0..2].copy_from_slice(&vr.fcw.to_ne_bytes());
                buf[2..4].copy_from_slice(&vr.fsw.to_ne_bytes());
                buf[4] = vr.ftw;
                // buf[5] = reserved (0)
                buf[6..8].copy_from_slice(&vr.fop.to_ne_bytes());
                // fip: FXSAVE stores as u32 fip + u16 fcs; extend to u64
                let fip64 = vr.fip as u64 | ((vr.fcs as u64) << 32);
                buf[8..16].copy_from_slice(&fip64.to_ne_bytes());
                // fdp: FXSAVE stores as u32 fdp + u16 fds; extend to u64
                let fdp64 = vr.fdp as u64 | ((vr.fds as u64) << 32);
                buf[16..24].copy_from_slice(&fdp64.to_ne_bytes());
                // buf[24..32] = padding (align st to 16 bytes)
                // st[0..7]: mm registers (ST0-ST7 in 80-bit x87 format)
                // Each stored as { low: u64, high: u64 } at offset 32 + i*16
                for (i, mm) in vr.mm.iter().enumerate() {
                    let off = 32 + i * 16;
                    buf[off..off + 8].copy_from_slice(&mm.0[0].to_ne_bytes());
                    buf[off + 8..off + 16].copy_from_slice(&mm.0[1].to_ne_bytes());
                }
                Ok(FP_SIZE)
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
            #[allow(unreachable_patterns)]
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
            #[cfg(target_arch = "x86_64")]
            ThreadStateKind::FloatPoint => {
                const FP_SIZE: usize = 160;
                if buf.len() < FP_SIZE {
                    return Err(ZxError::BUFFER_TOO_SMALL);
                }
                let vr = &mut self.vector_regs;
                vr.fcw = u16::from_ne_bytes(buf[0..2].try_into().unwrap());
                vr.fsw = u16::from_ne_bytes(buf[2..4].try_into().unwrap());
                vr.ftw = buf[4];
                vr.fop = u16::from_ne_bytes(buf[6..8].try_into().unwrap());
                let fip64 = u64::from_ne_bytes(buf[8..16].try_into().unwrap());
                vr.fip = fip64 as u32;
                vr.fcs = (fip64 >> 32) as u16;
                let fdp64 = u64::from_ne_bytes(buf[16..24].try_into().unwrap());
                vr.fdp = fdp64 as u32;
                vr.fds = (fdp64 >> 32) as u16;
                // buf[24..32] = padding (skipped)
                // st[0..7] at offset 32
                for (i, mm) in vr.mm.iter_mut().enumerate() {
                    let off = 32 + i * 16;
                    mm.0[0] = u64::from_ne_bytes(buf[off..off + 8].try_into().unwrap());
                    mm.0[1] = u64::from_ne_bytes(buf[off + 8..off + 16].try_into().unwrap());
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
                    let addr = u64::from_ne_bytes(buf[i * 8..(i + 1) * 8].try_into().unwrap());
                    // DR0-DR3 must be user-space addresses (or zero).
                    // Kernel-space and non-canonical addresses are rejected.
                    if addr != 0 {
                        let canonical = ((addr as i64) << 16 >> 16) as u64;
                        if addr != canonical || addr >= 0x0000_8000_0000_0000 {
                            return Err(ZxError::INVALID_ARGS);
                        }
                    }
                    dr.dr[i] = addr;
                }
                // DR6: bits 4-11 are reserved (always 1), bit 12 is reserved
                // (always 0). All other bits are user-writable.
                // Fuchsia masks: OR 0xFFFF0FF0 (set reserved 1-bits),
                // AND ~0x1000 (clear reserved 0-bit 12).
                let user_dr6 = u64::from_ne_bytes(buf[32..40].try_into().unwrap());
                // DR6 is a 32-bit register — mask to 32 bits, then apply
                // reserved bit rules: bits 4-11 always 1, bit 12 always 0.
                dr.dr6 = ((user_dr6 as u32 | 0xFFFF_0FF0) & !0x1000) as u64;
                // DR7: Fuchsia sets LE and GE bits (8-10) and clears
                // reserved bits 11-12. DR7 is a 32-bit register.
                let user_dr7 = u64::from_ne_bytes(buf[40..48].try_into().unwrap());
                dr.dr7 = ((user_dr7 as u32 | (0x7 << 8)) & !0xF800) as u64; // set 8-10, clear 11-15
            }
            #[allow(unreachable_patterns)]
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
