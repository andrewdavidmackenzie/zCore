#[derive(Debug, Default, Clone, Copy)]
#[repr(C)]
pub struct ContextData {
    // RFLAGS (including the interrupt flag). Saved/restored by the
    // context switch: a resumed context must run with ITS OWN
    // interrupt state, not the switcher's. Before this field existed,
    // a task parked at an await (IF=1) and resumed by the runtime
    // (which switches with IF=0 — see run_until_idle's lock
    // discipline) inherited IF=0 and kept executing kernel code with
    // interrupts disabled: the timer could not fire and any contended
    // KernelMutex escalated to the mutex.rs nested-spin panic (#862,
    // the multi-CPU panic cascade during process-crash teardown).
    pub rflags: usize,
    // pg base register
    pub cr3: usize,
    // callee saved registers
    pub r15: usize,
    pub r14: usize,
    pub r13: usize,
    pub r12: usize,
    pub rbp: usize,
    pub rbx: usize,
    // pc
    pub rip: usize,
}

impl ContextData {
    pub fn new(rip: usize, _sp: usize, cr3: usize) -> Self {
        Self {
            rip,
            cr3,
            // Initial contexts (executor entries) run with interrupts
            // enabled: they execute task polls, which are ordinary
            // task-context kernel code.
            rflags: 0x202,
            ..ContextData::default()
        }
    }

    /// Program counter.
    pub fn pc(&self) -> usize {
        self.rip
    }

    /// Page table base register (CR3).
    pub fn pgbr(&self) -> usize {
        self.cr3
    }
}
