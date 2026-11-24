//! Kernel handler trait.
//!
//! Functions implemented in the kernel and called back by the HAL
//! (dependency inversion for frame allocation and page fault handling).

use crate::{MMUFlags, PhysAddr, VirtAddr};

/// Information about an unrecoverable kernel-mode trap.
///
/// Passed to [`KernelHandler::handle_kernel_trap_fatal`] after the
/// HAL has already dumped the full trap frame to the early console.
/// The handler should kill the current thread (if any) so the rest
/// of the system observes the death; the calling CPU then halts.
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
    /// MSR access) was attempted. The kernel should kill the current
    /// thread if there is one — waiters are woken, death signals fire,
    /// crashsvc can process the exception — so the rest of the system
    /// degrades gracefully. The calling CPU halts afterwards (there is
    /// no safe way to unwind a kernel-mode fault through possibly-held
    /// locks), but every other CPU keeps running.
    fn handle_kernel_trap_fatal(&self, _info: &KernelTrapInfo) {
        // do nothing by default
    }
}
