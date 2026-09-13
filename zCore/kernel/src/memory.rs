//! Define dynamic memory allocation.

use crate::platform::phys_to_virt_offset;
use alloc::alloc::handle_alloc_error;
use core::{
    alloc::{GlobalAlloc, Layout},
    num::NonZeroUsize,
    ops::Range,
    ptr::NonNull,
};
use customizable_buddy::{BuddyAllocator, LinkedListBuddy, UsizeBuddy};
use kernel_hal::PhysAddr;
use lock::Mutex;

/// Heap allocator.
///
/// 27 + 6 + 3 = 36 -> 64 GiB
struct LockedHeap(Mutex<BuddyAllocator<27, UsizeBuddy, LinkedListBuddy>>);

#[global_allocator]
static HEAP: LockedHeap = LockedHeap(Mutex::new(BuddyAllocator::new()));

/// Number of page-offset bits (log2 of page size).
const PAGE_BITS: usize = 12;

/// Initial memory reserved for boot.
///
/// Tested memory requirements for different hardware:
///
/// | machine         | memory
/// | --------------- | -
/// | qemu,virt SMP 1 |  16 KiB
/// | qemu,virt SMP 4 |  32 KiB
/// | allwinner,nezha | 256 KiB
static mut MEMORY: [u8; 2 * 1024 * 1024] = [0u8; 2 * 1024 * 1024];

unsafe impl GlobalAlloc for LockedHeap {
    #[inline]
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if let Ok((ptr, _)) = self.0.lock().allocate_layout(layout) {
            ptr.as_ptr()
        } else {
            handle_alloc_error(layout)
        }
    }

    #[inline]
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        self.0
            .lock()
            .deallocate_layout(NonNull::new(ptr).unwrap(), layout)
    }
}

/// Initialize the allocator and register a small memory block for dynamic memory needed during boot.
pub fn init() {
    unsafe {
        log::info!("MEMORY = {:#?}", MEMORY.as_ptr_range());
        let mut heap = HEAP.0.lock();
        let ptr = NonNull::new(MEMORY.as_mut_ptr()).unwrap();
        heap.init(core::mem::size_of::<usize>().trailing_zeros() as _, ptr);
        heap.transfer(ptr, MEMORY.len());
    }
}

/// Register memory regions with the allocator.
pub fn insert_regions(regions: &[Range<PhysAddr>]) {
    let mut heap = HEAP.0.lock();
    let offset = phys_to_virt_offset();
    regions
        .iter()
        .filter(|region| !region.is_empty())
        .for_each(|region| unsafe {
            heap.transfer(
                NonNull::new_unchecked((region.start + offset) as *mut u8),
                region.len(),
            );
        });
}

pub fn frame_alloc(frame_count: usize, align_log2: usize) -> Option<PhysAddr> {
    let (ptr, size) = HEAP
        .0
        .lock()
        .allocate::<u8>(align_log2 << PAGE_BITS, unsafe {
            NonZeroUsize::new_unchecked(frame_count << PAGE_BITS)
        })
        .ok()?;
    assert_eq!(size, frame_count << PAGE_BITS);
    Some(ptr.as_ptr() as PhysAddr - phys_to_virt_offset())
}

pub fn frame_dealloc(target: PhysAddr) {
    HEAP.0.lock().deallocate(
        unsafe { NonNull::new_unchecked((target + phys_to_virt_offset()) as *mut u8) },
        1 << PAGE_BITS,
    );
}
