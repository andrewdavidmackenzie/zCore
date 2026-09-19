#[derive(Debug, Default, Clone, Copy)]
#[repr(C)]
pub struct ContextData {
    // callee saved registers
    pub s: [usize; 11],
    // pc / sp
    pub lr: usize,
    pub sp: usize,
    // pg base register
    pub ttbr0: usize,
}

impl ContextData {
    pub fn new(lr: usize, sp: usize, ttbr0: usize) -> Self {
        Self {
            s: [0; 11],
            lr,
            sp,
            ttbr0,
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
