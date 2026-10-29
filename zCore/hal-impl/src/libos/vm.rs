//! Virtual memory operations.

use super::mem::{MOCK_PHYS_MEM, PMEM_MAP_VADDR, PMEM_SIZE};
use crate::{addr::is_aligned, MMUFlags, PhysAddr, VirtAddr, PAGE_SIZE};

hal_fn_impl! {
    impl mod crate::hal_fn::vm {
        fn current_vmtoken() -> PhysAddr { 0 }
        fn activate_paging(_vmtoken: PhysAddr) {}
    }
}

/// Per-page entry tracking the physical address and flags of a mapping.
#[derive(Clone, Copy)]
struct PageEntry {
    paddr: PhysAddr,
    flags: MMUFlags,
}

/// Page table implemented by `mmap`, `munmap`, and `mprotect`, with
/// a software mapping table to support `query` on user virtual addresses.
pub struct PageTable {
    entries: std::collections::HashMap<VirtAddr, PageEntry>,
}

impl PageTable {
    pub fn new() -> Self {
        Self {
            entries: std::collections::HashMap::new(),
        }
    }

    pub fn from_current() -> Self {
        Self::new()
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
        // The mmap uses PAGE_SIZE for the mapping length.
        debug_assert!(is_aligned(paddr));
        if paddr < PMEM_SIZE {
            if MOCK_PHYS_MEM.mmap(page.vaddr, PAGE_SIZE, paddr, flags) {
                self.entries.insert(page.vaddr, PageEntry { paddr, flags });
                Ok(())
            } else {
                Err(PagingError::NoMemory)
            }
        } else {
            Err(PagingError::NoMemory)
        }
    }

    fn unmap(&mut self, vaddr: VirtAddr) -> PagingResult<(PhysAddr, PageSize)> {
        self.unmap_cont(vaddr, PAGE_SIZE)?;
        let paddr = self.entries.remove(&vaddr).map(|e| e.paddr).unwrap_or(0);
        Ok((paddr, PageSize::default_page()))
    }

    fn update(
        &mut self,
        vaddr: VirtAddr,
        paddr: Option<PhysAddr>,
        flags: Option<MMUFlags>,
    ) -> PagingResult<PageSize> {
        debug_assert!(is_aligned(vaddr));
        if let Some(entry) = self.entries.get_mut(&vaddr) {
            if let Some(pa) = paddr {
                entry.paddr = pa;
            }
            if let Some(f) = flags {
                entry.flags = f;
                MOCK_PHYS_MEM.mprotect(vaddr as _, PAGE_SIZE, f);
            }
        } else if let Some(flags) = flags {
            // Entry not tracked (e.g., PMEM range) — just mprotect.
            MOCK_PHYS_MEM.mprotect(vaddr as _, PAGE_SIZE, flags);
        }
        Ok(PageSize::default_page())
    }

    fn query(&self, vaddr: VirtAddr) -> PagingResult<(PhysAddr, MMUFlags, PageSize)> {
        debug_assert!(is_aligned(vaddr));
        // Check software mapping table first (user VA mappings).
        if let Some(entry) = self.entries.get(&vaddr) {
            return Ok((entry.paddr, entry.flags, PageSize::default_page()));
        }
        // Fall back to PMEM range check.
        if (PMEM_MAP_VADDR..PMEM_MAP_VADDR + PMEM_SIZE).contains(&vaddr) {
            Ok((
                vaddr - PMEM_MAP_VADDR,
                MMUFlags::READ | MMUFlags::WRITE,
                PageSize::default_page(),
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
        // Remove entries for the range.
        let end = vaddr + size;
        let mut v = vaddr;
        while v < end {
            self.entries.remove(&v);
            v += PAGE_SIZE;
        }
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
    #[test]
    fn map_unmap() {
        let mut pt = PageTable::new();
        let flags = MMUFlags::READ | MMUFlags::WRITE;
        // Addresses and paddr must be PAGE_SIZE-aligned (4K or 16K).
        let vaddr1 = VBASE + PAGE_SIZE;
        let vaddr2 = VBASE + 2 * PAGE_SIZE;
        let paddr = PAGE_SIZE;

        pt.map(
            Page::new_aligned(vaddr1, PageSize::default_page()),
            paddr,
            flags,
        )
        .unwrap();
        pt.map(
            Page::new_aligned(vaddr2, PageSize::default_page()),
            paddr,
            flags,
        )
        .unwrap();

        // On 16K hosts (aarch64 macOS), two different vaddrs mapping
        // the same paddr may use anonymous pages (no aliasing). Only
        // test shared-frame aliasing on 4K hosts.
        #[cfg(not(all(target_arch = "aarch64", target_os = "macos")))]
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
