use crate::imp::config::*;
use crate::{addr::align_up, PhysAddr, KCONFIG};
use alloc::{vec, vec::Vec};
use core::ops::Range;

extern "C" {
    fn ekernel();
}

/// Cut a region to exclude an overlapping range.
fn cut_off(total: Range<PhysAddr>, cut: &Range<PhysAddr>) -> Vec<Range<PhysAddr>> {
    let mut regions = Vec::new();
    if cut.end <= total.start || total.end <= cut.start {
        regions.push(total);
    } else {
        if total.start < cut.start {
            regions.push(total.start..crate::addr::align_down(cut.start));
        }
        if cut.end < total.end {
            regions.push(align_up(cut.end)..total.end);
        }
    }
    regions
}

pub fn free_pmem_regions() -> Vec<Range<PhysAddr>> {
    let start = align_up(ekernel as *const () as usize & PHYS_ADDR_MASK);
    let end = crate::addr::align_down(super::phys_memory_end());

    let mut regions = alloc::vec::Vec::new();
    regions.push(start as PhysAddr..end as PhysAddr);

    // Exclude the DTB region
    if KCONFIG.dtb_paddr != 0 && KCONFIG.dtb_size != 0 {
        let dtb = KCONFIG.dtb_paddr..KCONFIG.dtb_paddr + KCONFIG.dtb_size;
        regions = regions.into_iter().flat_map(|r| cut_off(r, &dtb)).collect();
    }

    // Exclude the initrd region
    if let Some(initrd) = super::INITRD_REGION.as_ref() {
        regions = regions
            .into_iter()
            .flat_map(|r| cut_off(r, initrd))
            .collect();
    }

    let total: usize = regions.iter().map(|r| r.end - r.start).sum();
    log::info!(
        "Free physical memory: {} MiB ({} regions)",
        total >> 20,
        regions.len()
    );
    for r in &regions {
        log::info!(
            "  {:#x}..{:#x} ({} MiB)",
            r.start,
            r.end,
            (r.end - r.start) >> 20
        );
    }
    regions
}

/// Flush the physical frame.
pub fn frame_flush(_target: PhysAddr) {
    unimplemented!()
}
