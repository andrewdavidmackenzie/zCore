use super::*;

pub struct VMObjectSlice {
    /// Parent node.
    parent: Arc<dyn VMObjectTrait>,
    /// The offset from parent.
    offset: usize,
    /// The size in bytes (for regular slices).
    size: usize,
    /// True for REFERENCE children: len() and set_len() delegate
    /// to the parent, making the reference a transparent alias.
    is_reference: bool,
}

impl VMObjectSlice {
    pub fn new(parent: Arc<dyn VMObjectTrait>, offset: usize, size: usize) -> Arc<Self> {
        Arc::new(VMObjectSlice {
            parent,
            offset,
            size,
            is_reference: false,
        })
    }

    pub fn new_reference(parent: Arc<dyn VMObjectTrait>, offset: usize, size: usize) -> Arc<Self> {
        Arc::new(VMObjectSlice {
            parent,
            offset,
            size,
            is_reference: true,
        })
    }

    fn check_range(&self, offset: usize, len: usize) -> ZxResult {
        // Use self.len() instead of self.size so reference slices
        // reflect the parent's current size after resizing.
        // Use > (not >=): a range ending exactly at size is valid.
        if !offset.checked_add(len).is_some_and(|end| end <= self.len()) {
            return Err(ZxError::OUT_OF_RANGE);
        }
        Ok(())
    }
}

impl VMObjectTrait for VMObjectSlice {
    fn read(&self, offset: usize, buf: &mut [u8]) -> ZxResult {
        self.check_range(offset, buf.len())?;
        self.parent.read(offset + self.offset, buf)
    }

    fn write(&self, offset: usize, buf: &[u8]) -> ZxResult {
        self.check_range(offset, buf.len())?;
        self.parent.write(offset + self.offset, buf)
    }

    fn zero(&self, offset: usize, len: usize) -> ZxResult {
        self.check_range(offset, len)?;
        self.parent.zero(offset + self.offset, len)
    }

    fn len(&self) -> usize {
        if self.is_reference {
            // REFERENCE children always reflect the parent's size.
            self.parent.len()
        } else {
            self.size
        }
    }

    fn set_len(&self, len: usize) -> ZxResult {
        if self.is_reference {
            // Delegate resize to the parent VMO.
            self.parent.set_len(len)
        } else {
            Err(ZxError::ACCESS_DENIED)
        }
    }

    fn commit_page(&self, page_idx: usize, flags: MMUFlags) -> ZxResult<usize> {
        self.parent
            .commit_page(page_idx + self.offset / PAGE_SIZE, flags)
    }

    fn commit_pages_with(
        &self,
        f: &mut dyn FnMut(&mut dyn FnMut(usize, MMUFlags) -> ZxResult<PhysAddr>) -> ZxResult,
    ) -> ZxResult {
        self.parent.commit_pages_with(f)
    }

    fn commit(&self, offset: usize, len: usize) -> ZxResult {
        self.parent.commit(offset + self.offset, len)
    }

    fn decommit(&self, offset: usize, len: usize) -> ZxResult {
        self.parent.decommit(offset + self.offset, len)
    }

    fn create_child(&self, _offset: usize, _len: usize) -> ZxResult<Arc<dyn VMObjectTrait>> {
        // Slices cannot create COW children at the trait level.
        // For REFERENCE children, the syscall handler detects the
        // reference and delegates to the parent VmObject's
        // create_child instead (which handles the COW tree correctly).
        Err(ZxError::NOT_SUPPORTED)
    }

    fn complete_info(&self, info: &mut VmoInfo) {
        // Inherit flags (e.g., CONTIGUOUS) from the parent, then
        // override page attribution to zero. Slices are transparent
        // windows -- pages are always attributed to the parent VMO,
        // not the slice.
        self.parent.complete_info(info);
        info.committed_bytes = 0;
        info.populated_bytes = 0;
        info.committed_private_bytes = 0;
        info.populated_private_bytes = 0;
        info.committed_scaled_bytes = 0;
        info.populated_scaled_bytes = 0;
        info.committed_fractional_scaled_bytes = 0;
        info.populated_fractional_scaled_bytes = 0;
    }

    fn is_reference(&self) -> bool {
        self.is_reference
    }

    fn parent_offset(&self) -> usize {
        self.offset
    }

    fn cache_policy(&self) -> CachePolicy {
        self.parent.cache_policy()
    }

    fn set_cache_policy(&self, _policy: CachePolicy) -> ZxResult {
        Ok(())
    }

    fn committed_pages_in_range(&self, start_idx: usize, end_idx: usize) -> usize {
        let po = pages(self.offset);
        self.parent
            .committed_pages_in_range(start_idx + po, end_idx + po)
    }

    fn pin(&self, offset: usize, len: usize) -> ZxResult {
        self.check_range(offset, len)?;
        self.parent.pin(offset + self.offset, len)
    }

    fn unpin(&self, offset: usize, len: usize) -> ZxResult {
        self.check_range(offset, len)?;
        self.parent.unpin(offset + self.offset, len)
    }

    fn is_contiguous(&self) -> bool {
        self.parent.is_contiguous()
    }

    fn is_paged(&self) -> bool {
        self.parent.is_paged()
    }
}
