//! Virtual memory operations.

use super::mem::{MOCK_PHYS_MEM, PMEM_MAP_VADDR, PMEM_SIZE};
use crate::{addr::is_aligned, MMUFlags, PhysAddr, VirtAddr, PAGE_SIZE};

hal_fn_impl! {
    impl mod crate::hal_fn::vm {
        fn current_vmtoken() -> PhysAddr { 0 }
        fn activate_paging(_vmtoken: PhysAddr) {}
    }
}

/// Dummy page table implemented by `mmap`, `munmap`, and `mprotect`.
pub struct PageTable;

impl PageTable {
    pub fn new() -> Self {
        Self
    }

    pub fn from_current() -> Self {
        Self
    }

    pub fn clone_kernel(&self) -> Self {
        Self::new()
    }
}

impl Default for PageTable {
    fn default() -> Self {
        Self::new()
    }
}

impl GenericPageTable for PageTable {
    fn table_phys(&self) -> PhysAddr {
        0
    }

    fn map(&mut self, page: Page, paddr: PhysAddr, flags: MMUFlags) -> PagingResult {
        debug_assert!(page.size as usize == PAGE_SIZE);
        debug_assert!(is_aligned(paddr));
        if paddr < PMEM_SIZE {
            MOCK_PHYS_MEM.mmap(page.vaddr, PAGE_SIZE, paddr, flags);
            Ok(())
        } else {
            Err(PagingError::NoMemory)
        }
    }

    fn unmap(&mut self, vaddr: VirtAddr) -> PagingResult<(PhysAddr, PageSize)> {
        self.unmap_cont(vaddr, PAGE_SIZE)?;
        Ok((0, PageSize::Size4K))
    }

    fn update(
        &mut self,
        vaddr: VirtAddr,
        _paddr: Option<PhysAddr>,
        flags: Option<MMUFlags>,
    ) -> PagingResult<PageSize> {
        debug_assert!(is_aligned(vaddr));
        if let Some(flags) = flags {
            MOCK_PHYS_MEM.mprotect(vaddr as _, PAGE_SIZE, flags);
        }
        Ok(PageSize::Size4K)
    }

    fn query(&self, vaddr: VirtAddr) -> PagingResult<(PhysAddr, MMUFlags, PageSize)> {
        debug_assert!(is_aligned(vaddr));
        if (PMEM_MAP_VADDR..PMEM_MAP_VADDR + PMEM_SIZE).contains(&vaddr) {
            Ok((
                vaddr - PMEM_MAP_VADDR,
                MMUFlags::READ | MMUFlags::WRITE,
                PageSize::Size4K,
            ))
        } else {
            Err(PagingError::NotMapped)
        }
    }

    fn unmap_cont(&mut self, vaddr: VirtAddr, size: usize) -> PagingResult {
        if size == 0 {
            return Ok(());
        }
        debug_assert!(is_aligned(vaddr));
        MOCK_PHYS_MEM.munmap(vaddr as _, size);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A valid virtual address base to mmap.
    /// On aarch64 macOS, mmap MAP_FIXED fails below ~0x400000000,
    /// so use a higher base address (16 GB instead of 8 GB).
    #[cfg(all(target_arch = "aarch64", target_os = "macos"))]
    const VBASE: VirtAddr = 0x0004_0000_0000;
    #[cfg(not(all(target_arch = "aarch64", target_os = "macos")))]
    const VBASE: VirtAddr = 0x0002_0000_0000;

    /// Test that two guest pages mapped to the same physical frame
    /// share data (write via one vaddr, read via the other).
    ///
    /// On 16K hosts, file-backed aliasing requires vaddr % hps == paddr % hps.
    /// We map paddr 0x1000 at VBASE+0x1000 and VBASE+0x5000 (both have
    /// vaddr % 16K == 0x1000 == paddr % 16K) in different host pages.
    #[test]
    fn map_unmap() {
        let mut pt = PageTable::new();
        let flags = MMUFlags::READ | MMUFlags::WRITE;
        // Use vaddrs where vaddr % host_page_size == paddr % host_page_size.
        // For paddr 0x1000, use VBASE+0x1000 and VBASE+0x5000 (16K apart,
        // both have offset 0x1000 within their host page).
        let vaddr1 = VBASE + 0x1000;
        let vaddr2 = VBASE + 0x5000; // 16K apart for different host pages
        let paddr = 0x1000;

        pt.map(Page::new_aligned(vaddr1, PageSize::Size4K), paddr, flags)
            .unwrap();
        pt.map(Page::new_aligned(vaddr2, PageSize::Size4K), paddr, flags)
            .unwrap();

        unsafe {
            const MAGIC: usize = 0xdead_beaf;
            (vaddr1 as *mut usize).write(MAGIC);
            assert_eq!(
                (vaddr2 as *mut usize).read(),
                MAGIC,
                "shared-frame aliasing: write at vaddr1 should be visible at vaddr2"
            );
        }

        pt.unmap(vaddr2).unwrap();
    }
}
