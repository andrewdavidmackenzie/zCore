#[derive(Debug, Default, Clone, Copy)]
#[repr(C)]
pub struct ContextData {
    // callee saved registers (x19-x29)
    pub s: [usize; 11],
    // pc / sp
    pub lr: usize,
    pub sp: usize,
    // pg base register
    pub ttbr0: usize,
    // FPU/SIMD state: 32 × 128-bit Q registers (q0-q31)
    pub fpu_regs: [u128; 32],
    // FPU control/status registers
    pub fpcr: u32,
    pub fpsr: u32,
}

impl ContextData {
    pub fn new(lr: usize, sp: usize, ttbr0: usize) -> Self {
        Self {
            s: [0; 11],
            lr,
            sp,
            ttbr0,
            fpu_regs: [0; 32],
            fpcr: 0,
            fpsr: 0,
        }
    }

    /// Program counter (link register).
    pub fn pc(&self) -> usize {
        self.lr
    }

    /// Stack pointer.
    pub fn sp(&self) -> usize {
        self.sp
    }

    /// Page table base register (TTBR0).
    pub fn pgbr(&self) -> usize {
        self.ttbr0
    }
}
