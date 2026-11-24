//! Kernel handler trait.
//!
//! Functions implemented in the kernel and called back by the HAL
//! (dependency inversion for frame allocation and page fault handling).

use crate::{MMUFlags, PhysAddr, VirtAddr};

/// Information about an unrecoverable kernel-mode trap.
///
/// Passed to [`KernelHandler::handle_kernel_trap_fatal`] after the
/// HAL has already dumped the full trap frame to the early console.
/// The handler should stage the current thread's death (if any) so
/// the rest of the system observes it; the HAL then either redirects
/// execution to the thread-exit path or halts the calling CPU.
pub struct KernelTrapInfo {
    /// Hardware trap/interrupt vector number.
    pub trap_num: usize,
    /// Architecture error code (0 if none).
    pub error_code: usize,
    /// Faulting instruction pointer.
    pub rip: usize,
    /// Stack pointer at the trap.
    pub rsp: usize,
    /// CPU (hart/core) the trap occurred on.
    pub cpu: usize,
    /// Short reason string (e.g. "unhandled kernel trap").
    pub reason: &'static str,
}

/// Outcome of [`KernelHandler::handle_kernel_trap_fatal`].
pub enum KernelTrapOutcome {
    /// The current thread's death was staged (queued for a task-context
    /// reaper) while its executor was mid-poll. The HAL will abandon the
    /// faulted poll — switching to the runtime idle context without
    /// saving the faulted context — so this CPU keeps scheduling work.
    /// The faulted task is orphaned; see `executor::abandon_current_poll`.
    Redirect,
    /// No redirect is possible (no current thread, or the runtime state
    /// does not allow abandoning). The calling CPU must halt (with
    /// interrupts enabled so it still services IPIs); other CPUs keep
    /// running.
    Halt,
}

/// Functions implemented in the kernel and used by HAL functions.
pub trait KernelHandler: Send + Sync + 'static {
    /// Allocate contiguous physical frames.
    fn frame_alloc_contiguous(&self, _frame_count: usize, _align_log2: usize) -> Option<PhysAddr> {
        unimplemented!()
    }

    /// Allocate one physical frame (convenience wrapper).
    fn frame_alloc(&self) -> Option<PhysAddr> {
        self.frame_alloc_contiguous(1, 0)
    }

    /// Deallocate a physical frame.
    fn frame_dealloc(&self, _paddr: PhysAddr) {
        unimplemented!()
    }

    /// Handle kernel mode page fault.
    fn handle_page_fault(&self, _fault_vaddr: VirtAddr, _access_flags: MMUFlags) {
        // do nothing
    }

    /// Try to handle a page fault, returning true if resolved.
    ///
    /// Unlike `handle_page_fault`, this does not panic on failure.
    /// Used by the guarded user-copy path to attempt demand-paging
    /// before falling back to the recovery label.
    fn try_handle_page_fault(&self, _fault_vaddr: VirtAddr, _access_flags: MMUFlags) -> bool {
        false
    }

    /// Handle a user-mode trap that isn't a page fault or syscall.
    ///
    /// Called for #GP, #UD, and other exceptions from user code.
    /// The kernel should deliver a Zircon exception to the process.
    /// `trap_num` is the hardware trap/interrupt vector number.
    fn handle_user_trap(&self, _trap_num: usize, _error_code: usize) {
        // do nothing by default
    }

    /// Handle an unrecoverable kernel-mode trap.
    ///
    /// Called after the HAL dumped the full trap frame to the early
    /// console and after bounded recovery (e.g. skipping a faulting
    /// MSR access) was attempted. The kernel should stage the current
    /// thread's death if there is one — waiters are woken, death
    /// signals fire, crashsvc can process the exception — so the rest
    /// of the system degrades gracefully.
    ///
    /// **Lock discipline (#829/#862):** this runs in interrupt context
    /// with interrupts disabled (x86 interrupt gates clear IF on
    /// entry). It must NOT acquire a `KernelMutex`: a contended
    /// acquisition cannot yield there and escalates to the
    /// `mutex.rs` nested-spin panic — the historical multi-CPU panic
    /// cascade during process-crash teardown. Stage the death in a
    /// lock-free way and let a task-context reaper perform the real
    /// teardown.
    ///
    /// Returns [`KernelTrapOutcome::Redirect`] when the HAL should
    /// abandon the faulted poll (freeing this CPU to continue
    /// scheduling), or [`KernelTrapOutcome::Halt`] when the CPU
    /// cannot safely continue.
    fn handle_kernel_trap_fatal(&self, _info: &KernelTrapInfo) -> KernelTrapOutcome {
        KernelTrapOutcome::Halt
    }
}
