//! Memory for libos mode.
//!
//! LibOS uses the host OS allocator for heap. Frame allocation is
//! provided by mock_mem (mmap-backed physical memory simulation).
//! These functions provide the same API as the bare-metal memory
//! module so the kernel can call them unconditionally.

use core::ops::Range;
use hal::PhysAddr;

pub fn init() {}

pub fn insert_regions(_regions: &[Range<PhysAddr>]) {}

pub fn frame_alloc(frame_count: usize, align_log2: usize) -> Option<PhysAddr> {
    if frame_count == 1 && align_log2 == 0 {
        super::mem::frame_alloc()
    } else {
        super::mem::frame_alloc_contiguous(frame_count, align_log2)
    }
}

pub fn frame_dealloc(paddr: PhysAddr) {
    super::mem::frame_dealloc(paddr)
}
