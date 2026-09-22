use hal_impl::{KernelHandler, MMUFlags};
use zircon_object::task::Thread;

pub struct ZcoreKernelHandler;

impl KernelHandler for ZcoreKernelHandler {
    fn frame_alloc_contiguous(&self, frame_count: usize, align_log2: usize) -> Option<usize> {
        hal_impl::memory::frame_alloc(frame_count, align_log2)
    }

    fn frame_dealloc(&self, paddr: usize) {
        hal_impl::memory::frame_dealloc(paddr);
    }

    fn handle_page_fault(&self, fault_vaddr: usize, access_flags: MMUFlags) {
        if let Some(thread) = hal_impl::thread::get_current_thread() {
            let thread = thread.downcast::<Thread>().unwrap();
            let vmar = thread.proc().vmar();
            if let Err(err) = vmar.handle_page_fault(fault_vaddr, access_flags) {
                panic!(
                    "handle kernel page fault error: {:?} vaddr(0x{:x}) flags({:?})",
                    err, fault_vaddr, access_flags
                );
            }
        } else {
            panic!(
                "page fault from kernel private address 0x{:x}, flags = {:?}",
                fault_vaddr, access_flags
            );
        }
    }
}
