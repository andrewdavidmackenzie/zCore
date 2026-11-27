use {super::*, alloc::sync::Arc, lock::KernelMutex};

/// VMO representing a physical range of memory.
pub struct VMObjectPhysical {
    paddr: PhysAddr,
    pages: usize,
    /// Lock this when access physical memory.
    data_lock: KernelMutex<()>,
    inner: KernelMutex<VMObjectPhysicalInner>,
}

struct VMObjectPhysicalInner {
    cache_policy: CachePolicy,
}

impl VMObjectPhysicalInner {
    pub fn new() -> VMObjectPhysicalInner {
        VMObjectPhysicalInner {
            cache_policy: CachePolicy::Uncached,
        }
    }
}

impl VMObjectPhysical {
    /// Create a new VMO representing a piece of contiguous physical memory.
    /// You must ensure nobody has the ownership of this piece of memory yet.
    pub fn new(paddr: PhysAddr, pages: usize) -> Arc<Self> {
        assert!(page_aligned(paddr.0));
        Arc::new(VMObjectPhysical {
            paddr,
            pages,
            data_lock: KernelMutex::default(),
            inner: KernelMutex::new(VMObjectPhysicalInner::new()),
        })
    }
}

impl VMObjectTrait for VMObjectPhysical {
    fn read(&self, offset: usize, buf: &mut [u8]) -> ZxResult {
        let _ = self.data_lock.lock();
        assert!(offset + buf.len() <= self.len());
        hal_impl::mem::pmem_read(self.paddr + offset, buf);
        Ok(())
    }

    fn write(&self, offset: usize, buf: &[u8]) -> ZxResult {
        let _ = self.data_lock.lock();
        assert!(offset + buf.len() <= self.len());
        hal_impl::mem::pmem_write(self.paddr + offset, buf);
        Ok(())
    }

    fn zero(&self, offset: usize, len: usize) -> ZxResult {
        let _ = self.data_lock.lock();
        assert!(offset + len <= self.len());
        hal_impl::mem::pmem_zero(self.paddr + offset, len);
        Ok(())
    }

    fn len(&self) -> usize {
        self.pages * PAGE_SIZE
    }

    fn set_len(&self, _len: usize) -> ZxResult {
        // #538: physical VMOs cannot be resized (user-reachable via
        // zx_vmo_set_size) — NOT_SUPPORTED, not a crash.
        Err(ZxError::NOT_SUPPORTED)
    }

    fn commit_page(&self, page_idx: usize, _flags: MMUFlags) -> ZxResult<PhysAddr> {
        Ok(self.paddr + page_idx * PAGE_SIZE)
    }

    fn commit_pages_with(
        &self,
        f: &mut dyn FnMut(&mut dyn FnMut(usize, MMUFlags) -> ZxResult<PhysAddr>) -> ZxResult,
    ) -> ZxResult {
        f(&mut |page_idx, _flags| Ok(self.paddr + page_idx * PAGE_SIZE))
    }

    fn commit(&self, _offset: usize, _len: usize) -> ZxResult {
        // do nothing
        Ok(())
    }

    fn decommit(&self, _offset: usize, _len: usize) -> ZxResult {
        // do nothing
        Ok(())
    }

    fn create_child(&self, _offset: usize, _len: usize) -> ZxResult<Arc<dyn VMObjectTrait>> {
        Err(ZxError::NOT_SUPPORTED)
    }

    fn complete_info(&self, info: &mut VmoInfo) {
        info.flags |= VmoInfoFlags::TYPE_PHYSICAL | VmoInfoFlags::CONTIGUOUS;
        // Physical VMOs do not consume VMO memory — report zero
        // for committed/populated accounting. Mark fractional scaling
        // as unsupported (sentinel u64::MAX).
        info.committed_bytes = 0;
        info.populated_bytes = 0;
        info.populated_scaled_bytes = 0;
        info.populated_fractional_scaled_bytes = u64::MAX;
    }

    fn cache_policy(&self) -> CachePolicy {
        let inner = self.inner.lock();
        inner.cache_policy
    }

    fn set_cache_policy(&self, policy: CachePolicy) -> ZxResult {
        let mut inner = self.inner.lock();
        inner.cache_policy = policy;
        Ok(())
    }

    fn committed_pages_in_range(&self, _start_idx: usize, _end_idx: usize) -> usize {
        0
    }

    fn is_contiguous(&self) -> bool {
        true
    }
    fn is_physical(&self) -> bool {
        true
    }
}

#[cfg(test)]
#[serial_test::parallel]
mod tests {
    use super::*;
    use hal::CachePolicy;

    #[test]
    fn read_write() {
        // Allocate the physical window from the frame allocator so
        // this test owns it exclusively. The previous hardcoded paddr
        // (PAGE_SIZE = the allocator's frame 1) aliased every paged
        // VMO that held that frame: this test's writes clobbered
        // their pages and theirs clobbered this test's reads — the
        // cross-test data corruption behind #880/#890 (caught by
        // watching the victim's frame: a 4-byte pmem_write from this
        // test landed on the victim's page 0).
        let paddr = hal_impl::mem::frame_alloc_contiguous(2, 0)
            .expect("failed to allocate contiguous frames");
        let vmo = VmObject::new_physical(paddr, 2);
        assert_eq!(vmo.cache_policy(), CachePolicy::Uncached);
        super::super::tests::read_write(&vmo);
    }
}
