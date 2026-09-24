//! Physical frame allocation and heap (global allocator).
//!
//! This module provides the `#[global_allocator]` for bare-metal mode.
//! - aarch64/riscv64: buddy allocator for both heap and frame allocation
//! - x86_64: bitmap allocator for frames, buddy allocator for heap

use core::ops::Range;
use hal::PhysAddr;
use lock::Mutex;

const PAGE_BITS: usize = 12;

// ============================================================
// aarch64 / riscv64: unified buddy allocator
// ============================================================
#[cfg(not(target_arch = "x86_64"))]
mod buddy {
    use super::*;
    use alloc::alloc::handle_alloc_error;
    use core::{
        alloc::{GlobalAlloc, Layout},
        num::NonZeroUsize,
        ptr::NonNull,
    };
    use customizable_buddy::{BuddyAllocator, LinkedListBuddy, UsizeBuddy};

    /// Heap allocator (27 + 6 + 3 = 36 -> 64 GiB).
    struct LockedHeap(Mutex<BuddyAllocator<27, UsizeBuddy, LinkedListBuddy>>);

    #[global_allocator]
    static HEAP: LockedHeap = LockedHeap(Mutex::new(BuddyAllocator::new()));

    /// Initial memory reserved for boot (4 MiB).
    /// Page-aligned to satisfy the buddy allocator's minimum order
    /// requirement (min_order = 3 on 64-bit, i.e. 8-byte aligned).
    /// Without this, the linker may place the array at an odd address
    /// (seen on riscv64), causing an underflow in the buddy allocator.
    /// 4 MiB is enough for page table allocation when mapping ~1 GiB
    /// of physical memory with 4K pages (~512 L3 page tables = 2 MiB).
    #[repr(C, align(4096))]
    struct AlignedMemory([u8; 4 * 1024 * 1024]);
    static mut MEMORY: AlignedMemory = AlignedMemory([0u8; 4 * 1024 * 1024]);

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

    pub fn init() {
        unsafe {
            log::info!("MEMORY = {:#?}", MEMORY.0.as_ptr_range());
            let mut heap = HEAP.0.lock();
            let ptr = NonNull::new(MEMORY.0.as_mut_ptr()).unwrap();
            heap.init(core::mem::size_of::<usize>().trailing_zeros() as _, ptr);
            heap.transfer(ptr, MEMORY.0.len());
        }
    }

    pub fn insert_regions(regions: &[Range<PhysAddr>]) {
        use crate::hal_fn::mem::{phys_to_virt, virt_to_phys};
        let _ = virt_to_phys; // suppress unused warning
        let mut heap = HEAP.0.lock();
        regions
            .iter()
            .filter(|region| !region.is_empty())
            .for_each(|region| unsafe {
                heap.transfer(
                    NonNull::new_unchecked(phys_to_virt(region.start) as *mut u8),
                    region.len(),
                );
            });
    }

    pub fn frame_alloc(frame_count: usize, align_log2: usize) -> Option<PhysAddr> {
        use crate::hal_fn::mem::virt_to_phys;
        #[cfg(feature = "uefi-boot")]
        unsafe {
            let u = 0xffff_0000_0900_0000 as *mut u8;
            core::ptr::write_volatile(u, b'A');
        }
        let (ptr, size) = HEAP
            .0
            .lock()
            .allocate::<u8>(align_log2 << PAGE_BITS, unsafe {
                NonZeroUsize::new_unchecked(frame_count << PAGE_BITS)
            })
            .ok()?;
        assert_eq!(size, frame_count << PAGE_BITS);
        let paddr = virt_to_phys(ptr.as_ptr() as usize);
        #[cfg(feature = "uefi-boot")]
        unsafe {
            let u = 0xffff_0000_0900_0000 as *mut u8;
            core::ptr::write_volatile(u, b'a');
        }
        Some(paddr)
    }

    pub fn frame_dealloc(target: PhysAddr) {
        use crate::hal_fn::mem::phys_to_virt;
        HEAP.0.lock().deallocate(
            unsafe { NonNull::new_unchecked(phys_to_virt(target) as *mut u8) },
            1 << PAGE_BITS,
        );
    }
}

// ============================================================
// x86_64: bitmap frame allocator + buddy heap allocator
// ============================================================
#[cfg(target_arch = "x86_64")]
mod bitmap {
    use super::*;
    use bitmap_allocator::BitAlloc;
    use buddy_system_allocator::Heap;
    use core::{
        alloc::{GlobalAlloc, Layout},
        ops::Deref,
        ptr::NonNull,
    };

    type FrameAlloc = bitmap_allocator::BitAlloc16M; // max 64G

    /// Global physical frame allocator.
    static FRAME_ALLOCATOR: Mutex<FrameAlloc> = Mutex::new(FrameAlloc::DEFAULT);

    /// Physical page reserved for the SMP AP trampoline.
    const SMP_TRAMPOLINE_PAGE: usize = 0x8000 >> PAGE_BITS;

    const KERNEL_HEAP_SIZE: usize = 16 * 1024 * 1024; // 16 MB
    const ORDER: usize = 32;

    #[global_allocator]
    static HEAP_ALLOCATOR: LockedHeap<ORDER> = LockedHeap::<ORDER>::new();

    pub fn init() {
        const MACHINE_ALIGN: usize = core::mem::size_of::<usize>();
        const HEAP_BLOCK: usize = KERNEL_HEAP_SIZE / MACHINE_ALIGN;
        static mut HEAP: [usize; HEAP_BLOCK] = [0; HEAP_BLOCK];
        let heap_start = unsafe { HEAP.as_ptr() as usize };
        unsafe {
            HEAP_ALLOCATOR
                .lock()
                .init(heap_start, HEAP_BLOCK * MACHINE_ALIGN);
        }
        log::info!(
            "Heap init end: {:#x?}",
            heap_start..heap_start + KERNEL_HEAP_SIZE
        );
    }

    pub fn insert_regions(regions: &[Range<PhysAddr>]) {
        log::debug!("init_frame_allocator regions: {regions:x?}");
        let mut ba = FRAME_ALLOCATOR.lock();
        for region in regions {
            if region.is_empty() {
                continue;
            }
            let frame_start = region.start >> PAGE_BITS;
            let frame_end = ((region.end - 1) >> PAGE_BITS) + 1;
            if frame_start < frame_end {
                ba.insert(frame_start..frame_end);
                log::trace!(
                    "Frame allocator: add range {:#x?}",
                    (frame_start << PAGE_BITS)..(frame_end << PAGE_BITS),
                );
            }
        }
        // Reserve the SMP trampoline page.
        ba.remove(SMP_TRAMPOLINE_PAGE..SMP_TRAMPOLINE_PAGE + 1);
        log::info!("Frame allocator init end.");
    }

    pub fn frame_alloc(frame_count: usize, align_log2: usize) -> Option<PhysAddr> {
        use bitmap_allocator::BitAlloc;
        let ret = FRAME_ALLOCATOR
            .lock()
            .alloc_contiguous(None, frame_count, align_log2)
            .map(|idx| idx << PAGE_BITS);
        log::trace!(
            "frame_alloc_contiguous(): {ret:x?} ~ {end_ret:x?}, align_log2={align_log2}",
            end_ret = ret.map(|x| x + frame_count),
        );
        ret
    }

    pub fn frame_dealloc(target: PhysAddr) {
        use bitmap_allocator::BitAlloc;
        log::trace!("frame_dealloc(): {target:x}");
        let _ = FRAME_ALLOCATOR.lock().dealloc(target >> PAGE_BITS);
    }

    struct LockedHeap<const ORDER: usize>(Mutex<Heap<ORDER>>);

    impl<const ORDER: usize> LockedHeap<ORDER> {
        pub const fn new() -> Self {
            LockedHeap(Mutex::new(Heap::<ORDER>::new()))
        }
    }

    impl<const ORDER: usize> Deref for LockedHeap<ORDER> {
        type Target = Mutex<Heap<ORDER>>;
        fn deref(&self) -> &Self::Target {
            &self.0
        }
    }

    unsafe impl<const ORDER: usize> GlobalAlloc for LockedHeap<ORDER> {
        unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
            self.0
                .lock()
                .alloc(layout)
                .ok()
                .map_or(core::ptr::null_mut::<u8>(), |allocation| {
                    allocation.as_ptr()
                })
        }

        unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
            self.0.lock().dealloc(NonNull::new_unchecked(ptr), layout)
        }
    }
}

// ============================================================
// Public API (delegates to the arch-specific module)
// ============================================================

#[cfg(not(target_arch = "x86_64"))]
pub use buddy::{frame_alloc, frame_dealloc, init, insert_regions};

#[cfg(target_arch = "x86_64")]
pub use bitmap::{frame_alloc, frame_dealloc, init, insert_regions};
