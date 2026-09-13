//! Global allocator for petal programs.
//!
//! Simple bump allocator using a static buffer. Sufficient for
//! short-lived programs that don't need to free memory.

use core::alloc::{GlobalAlloc, Layout};
use core::sync::atomic::{AtomicUsize, Ordering};

/// Heap size (256 KiB).
const HEAP_SIZE: usize = 256 * 1024;

/// Static heap buffer. Placed in .bss (zero-initialized).
#[repr(C, align(16))]
struct HeapBuf([u8; HEAP_SIZE]);

static mut HEAP: HeapBuf = HeapBuf([0; HEAP_SIZE]);

/// Bump allocator over the static heap buffer.
struct PetalAlloc {
    next: AtomicUsize,
    end: AtomicUsize,
}

#[global_allocator]
static ALLOCATOR: PetalAlloc = PetalAlloc {
    next: AtomicUsize::new(0),
    end: AtomicUsize::new(0),
};

/// Initialize the heap. Called from `_start` before `main`.
pub fn init_heap() {
    let base = core::ptr::addr_of!(HEAP) as usize;
    ALLOCATOR.next.store(base, Ordering::SeqCst);
    ALLOCATOR.end.store(base + HEAP_SIZE, Ordering::SeqCst);
}

unsafe impl GlobalAlloc for PetalAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        loop {
            let current = self.next.load(Ordering::Relaxed);
            let aligned = (current + layout.align() - 1) & !(layout.align() - 1);
            let new_next = aligned + layout.size();

            if new_next > self.end.load(Ordering::Relaxed) {
                return core::ptr::null_mut();
            }

            if self
                .next
                .compare_exchange_weak(current, new_next, Ordering::SeqCst, Ordering::Relaxed)
                .is_ok()
            {
                return aligned as *mut u8;
            }
        }
    }

    unsafe fn dealloc(&self, _ptr: *mut u8, _layout: Layout) {
        // Bump allocator: no deallocation.
    }
}
