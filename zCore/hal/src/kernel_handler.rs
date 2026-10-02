//! Kernel handler trait.
//!
//! Functions implemented in the kernel and called back by the HAL
//! (dependency inversion for frame allocation and page fault handling).

use crate::{MMUFlags, PhysAddr, VirtAddr};

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

    /// Handle a user-mode trap that isn't a page fault or syscall.
    ///
    /// Called for #GP, #UD, and other exceptions from user code.
    /// The kernel should deliver a Zircon exception to the process.
    /// `trap_num` is the hardware trap/interrupt vector number.
    fn handle_user_trap(&self, _trap_num: usize, _error_code: usize) {
        // do nothing by default
    }
}
