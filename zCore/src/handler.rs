use kernel_hal::{KernelHandler, MMUFlags};
use zircon_object::task::Thread;

#[cfg(not(feature = "libos"))]
use super::memory;

pub struct ZcoreKernelHandler;

impl KernelHandler for ZcoreKernelHandler {
    fn frame_alloc(&self) -> Option<usize> {
        #[cfg(not(feature = "libos"))]
        {
            memory::frame_alloc(1, 0)
        }
        #[cfg(feature = "libos")]
        {
            // In libos mode, physical frame allocation is handled by the host.
            // Return a mock address from the host's malloc.
            let layout = core::alloc::Layout::from_size_align(0x1000, 0x1000).unwrap();
            let ptr = unsafe { alloc::alloc::alloc_zeroed(layout) };
            if ptr.is_null() {
                None
            } else {
                Some(ptr as usize)
            }
        }
    }

    fn frame_alloc_contiguous(&self, frame_count: usize, align_log2: usize) -> Option<usize> {
        #[cfg(not(feature = "libos"))]
        {
            memory::frame_alloc(frame_count, align_log2)
        }
        #[cfg(feature = "libos")]
        {
            let size = frame_count.checked_mul(0x1000)?;
            let align = 1usize.checked_shl(align_log2.max(12) as u32)?;
            let layout = core::alloc::Layout::from_size_align(size, align).ok()?;
            let ptr = unsafe { alloc::alloc::alloc_zeroed(layout) };
            if ptr.is_null() {
                None
            } else {
                Some(ptr as usize)
            }
        }
    }

    fn frame_dealloc(&self, paddr: usize) {
        #[cfg(not(feature = "libos"))]
        memory::frame_dealloc(paddr);
        #[cfg(feature = "libos")]
        {
            // In libos mode, frames are host-allocated pages. Deallocate
            // a single page. Multi-page contiguous allocations are not
            // individually tracked, so we deallocate one page at a time
            // (matching the single-page frame_alloc pattern).
            let layout = core::alloc::Layout::from_size_align(0x1000, 0x1000).unwrap();
            unsafe {
                alloc::alloc::dealloc(paddr as *mut u8, layout);
            }
        }
    }

    fn handle_page_fault(&self, fault_vaddr: usize, access_flags: MMUFlags) {
        if let Some(thread) = kernel_hal::thread::get_current_thread() {
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
