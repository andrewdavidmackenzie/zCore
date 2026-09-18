use crate::imp::config::*;
use crate::PhysAddr;
use alloc::vec::Vec;
use core::ops::Range;

extern "C" {
    fn ekernel();
}

pub fn free_pmem_regions() -> Vec<Range<PhysAddr>> {
    let mut regions = Vec::new();
    let start = ekernel as *const () as usize & PHYS_ADDR_MASK;
    let end = super::phys_memory_end();
    log::info!(
        "Free physical memory: {:#x}..{:#x} ({} MiB)",
        start,
        end,
        (end - start) >> 20
    );
    regions.push(start as PhysAddr..end as PhysAddr);
    regions
}

/// Flush the physical frame.
pub fn frame_flush(_target: PhysAddr) {
    unimplemented!()
}
