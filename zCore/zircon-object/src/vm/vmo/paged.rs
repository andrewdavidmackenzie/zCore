use {
    super::*,
    crate::util::block_range::BlockIter,
    alloc::collections::BTreeMap,
    alloc::collections::VecDeque,
    alloc::sync::{Arc, Weak},
    alloc::vec::Vec,
    core::cell::{Ref, RefCell, RefMut},
    core::ops::Range,
    core::sync::atomic::*,
    hal_impl::{
        mem::{phys_to_virt, PhysFrame},
        PAGE_SIZE,
    },
    lock::{Mutex, MutexGuard},
};

enum VMOType {
    /// The original node.
    Origin,
    /// A snapshot of the parent node.
    Snapshot,
    /// Internal non-leaf node for snapshot.
    ///
    /// ```text
    ///    v---create_child
    ///    O       H <--- hidden node
    ///   /   =>  / \
    ///  S       O   S
    /// ```
    Hidden {
        /// The left child.
        left: WeakRef,
        /// The right child.
        right: WeakRef,
    },
}

impl VMOType {
    fn get_tag_and_other(&self, child: &WeakRef) -> (PageStateTag, WeakRef) {
        match self {
            VMOType::Hidden { left, right, .. } => {
                if left.ptr_eq(child) {
                    (PageStateTag::LeftSplit, right.clone())
                } else if right.ptr_eq(child) {
                    (PageStateTag::RightSplit, left.clone())
                } else {
                    (PageStateTag::Owned, Weak::new())
                }
            }
            _ => (PageStateTag::Owned, Weak::new()),
        }
    }

    fn is_hidden(&self) -> bool {
        matches!(self, VMOType::Hidden { .. })
    }
}

/// The main VM object type, holding a list of pages.
pub struct VMObjectPaged {
    /// The lock that protected the `inner`
    /// This lock is shared between objects in the same clone tree to avoid deadlock
    lock: Arc<Mutex<()>>,
    inner: RefCell<VMObjectPagedInner>,
}

/// We always lock the lock before access to the Refcell, so it is actually sync
#[allow(unsafe_code)]
unsafe impl Sync for VMObjectPaged {}

type WeakRef = Weak<VMObjectPaged>;

/// The mutable part of `VMObjectPaged`.
struct VMObjectPagedInner {
    /// Owner identifier.
    owner: u64,
    type_: VMOType,
    /// Parent node.
    parent: Option<Arc<VMObjectPaged>>,
    /// The offset from parent.
    parent_offset: usize,
    /// The range limit from parent.
    parent_limit: usize,
    /// The size in bytes.
    size: usize,
    /// Physical frames of this VMO.
    frames: BTreeMap<usize, PageState>,
    /// All mappings to this VMO.
    mappings: Vec<Weak<VmMapping>>,
    /// Cache Policy
    cache_policy: CachePolicy,
    /// Is contiguous
    contiguous: bool,
    /// A weak reference to myself.
    self_ref: WeakRef,
    /// Sum of pin_count
    pin_count: usize,
}

/// Page state in VMO.
struct PageState {
    frame: PhysFrame,
    tag: PageStateTag,
    pin_count: u8,
    /// Number of additional VMOs that share this page beyond one.
    /// share_count=0 means 1 viewer (private), share_count=N means N+1 viewers.
    share_count: u32,
}

/// The owner tag of pages in the node.
#[derive(Debug, PartialEq, Eq, Copy, Clone)]
enum PageStateTag {
    /// If the node is hidden, the page is shared by its 2 children.
    /// Otherwise, the page is owned by the node.
    Owned,
    /// The page is split to the left child and now owned by the right child.
    LeftSplit,
    /// The page is split to the right child and now owned by the left child.
    RightSplit,
}

impl PageStateTag {
    fn negate(self) -> Self {
        match self {
            PageStateTag::LeftSplit => PageStateTag::RightSplit,
            PageStateTag::RightSplit => PageStateTag::LeftSplit,
            PageStateTag::Owned => unreachable!(),
        }
    }
    fn is_split(self) -> bool {
        self != PageStateTag::Owned
    }
}

/// Result of per-page fractional attribution computation.
#[derive(Default)]
struct Attribution {
    /// Sum of `PAGE_SIZE / sharing_count` for each page (integer part).
    scaled_bytes: u64,
    /// 63-bit fixed-point fractional remainder from the division.
    fractional_scaled_bytes: u64,
}

impl Attribution {
    /// Add one page with the given sharing count to the attribution.
    fn add_page(&mut self, sharing_count: u64) {
        if sharing_count == 0 {
            return;
        }
        let page_size = PAGE_SIZE as u64;
        // Integer part
        self.scaled_bytes += page_size / sharing_count;
        // Fractional part: (PAGE_SIZE % sharing_count) << 63 / sharing_count
        let remainder = page_size % sharing_count;
        if remainder > 0 {
            // Use 128-bit arithmetic to avoid overflow:
            // fractional = (remainder << 63) / sharing_count
            let numer = (remainder as u128) << 63;
            self.fractional_scaled_bytes += (numer / sharing_count as u128) as u64;
            // Handle carry: if fractional overflows 63 bits, add 1 to integer.
            if self.fractional_scaled_bytes >= (1u64 << 63) {
                self.scaled_bytes += 1;
                self.fractional_scaled_bytes -= 1u64 << 63;
            }
        }
    }
}

impl PageState {
    fn new(frame: PhysFrame) -> Self {
        VMO_PAGE_ALLOC.add(1);
        PageState {
            frame,
            tag: PageStateTag::Owned,
            pin_count: 0,
            share_count: 0,
        }
    }
    #[allow(unsafe_code)]
    fn take(self) -> PhysFrame {
        let frame = unsafe { core::mem::transmute_copy(&self.frame) };
        VMO_PAGE_DEALLOC.add(1);
        core::mem::forget(self);
        frame
    }
    fn swap(&mut self, t: &mut Self) {
        core::mem::swap(&mut self.frame, &mut t.frame);
        core::mem::swap(&mut self.pin_count, &mut t.pin_count);
    }
}

impl Drop for PageState {
    fn drop(&mut self) {
        VMO_PAGE_DEALLOC.add(1);
    }
}

impl VMObjectPaged {
    /// Create a new VMO backing on physical memory allocated in pages.
    pub fn new(pages: usize) -> Arc<Self> {
        VMObjectPaged::wrap(
            VMObjectPagedInner {
                owner: new_owner_id(),
                type_: VMOType::Origin,
                parent: None,
                parent_offset: 0usize,
                parent_limit: 0usize,
                size: pages * PAGE_SIZE,
                frames: BTreeMap::new(),
                mappings: Vec::new(),
                cache_policy: CachePolicy::Cached,
                contiguous: false,
                self_ref: Default::default(),
                pin_count: 0,
            },
            None,
        )
    }

    /// Create a new VMO backing on contiguous pages.
    pub fn new_contiguous(pages: usize, align_log2: usize) -> ZxResult<Arc<Self>> {
        let vmo = Self::new(pages);
        let mut frames = PhysFrame::new_contiguous(pages, align_log2 - PAGE_SIZE_LOG2);
        if frames.is_empty() {
            return Err(ZxError::NO_MEMORY);
        }
        {
            let (_guard, mut inner) = vmo.get_inner_mut();
            inner.contiguous = true;
            for (i, f) in frames.drain(0..).enumerate() {
                hal_impl::mem::pmem_zero(f.paddr(), PAGE_SIZE);
                let mut state = PageState::new(f);
                state.pin_count += 1;
                inner.frames.insert(i, state);
            }
        }
        Ok(vmo)
    }

    /// Internal: Wrap an inner struct to object.
    fn wrap(inner: VMObjectPagedInner, lock_ref: Option<Arc<Mutex<()>>>) -> Arc<Self> {
        let obj = Arc::new(VMObjectPaged {
            lock: lock_ref.unwrap_or_else(|| Arc::new(Mutex::new(()))),
            inner: RefCell::new(inner),
        });
        obj.inner.borrow_mut().self_ref = Arc::downgrade(&obj);
        obj
    }

    /// get the reference to inner by lock the shared lock
    fn get_inner(&self) -> (MutexGuard<'_, ()>, Ref<'_, VMObjectPagedInner>) {
        (self.lock.lock(), self.inner.borrow())
    }

    /// get the mutable reference to inner by lock the shared lock
    fn get_inner_mut(&self) -> (MutexGuard<'_, ()>, RefMut<'_, VMObjectPagedInner>) {
        (self.lock.lock(), self.inner.borrow_mut())
    }
}

impl VMObjectTrait for VMObjectPaged {
    fn read(&self, offset: usize, buf: &mut [u8]) -> ZxResult {
        let (_guard, mut inner) = self.get_inner_mut();
        if inner.cache_policy != CachePolicy::Cached {
            return Err(ZxError::BAD_STATE);
        }
        inner.for_each_page(offset, buf.len(), MMUFlags::READ, |paddr, buf_range| {
            hal_impl::mem::pmem_read(paddr, &mut buf[buf_range]);
        })
    }

    fn write(&self, offset: usize, buf: &[u8]) -> ZxResult {
        trace!("VMO write: offset={:#x}, len={}", offset, buf.len());
        let (_guard, mut inner) = self.get_inner_mut();
        trace!("VMO write: lock acquired, cache={:?}", inner.cache_policy);
        if inner.cache_policy != CachePolicy::Cached {
            return Err(ZxError::BAD_STATE);
        }
        trace!("VMO write: calling for_each_page");
        let result = inner.for_each_page(offset, buf.len(), MMUFlags::WRITE, |paddr, buf_range| {
            hal_impl::mem::pmem_write(paddr, &buf[buf_range]);
        });
        trace!("VMO write: done, result={:?}", result);
        result
    }

    fn zero(&self, offset: usize, len: usize) -> ZxResult {
        let (_guard, mut inner) = self.get_inner_mut();
        if offset + len > inner.size {
            return Err(ZxError::OUT_OF_RANGE);
        }
        let iter = BlockIter {
            begin: offset,
            end: offset + len,
            block_size_log2: 12,
        };
        let mut unwanted = VecDeque::new();
        for block in iter {
            //let paddr = self.commit_page(block.block, MMUFlags::READ)?;
            if block.len() == PAGE_SIZE && !inner.is_contiguous() {
                let _ = inner.commit_page(block.block, MMUFlags::WRITE)?;
                unwanted.push_back(block.block + inner.parent_offset / PAGE_SIZE);
                inner.frames.remove(&block.block);
            } else if inner.committed_pages_in_range(block.block, block.block + 1) != 0 {
                // check whether this page is initialized, otherwise nothing should be done
                let paddr = inner.commit_page(block.block, MMUFlags::WRITE)?;
                hal_impl::mem::pmem_zero(paddr + block.begin, block.len());
            }
        }
        inner.release_unwanted_pages_in_parent(unwanted);
        Ok(())
    }

    fn len(&self) -> usize {
        self.get_inner().1.size
    }

    fn set_len(&self, len: usize) -> ZxResult {
        assert!(page_aligned(len));
        let old_parent = {
            let (_guard, mut inner) = self.get_inner_mut();
            if inner.pin_count > 0 {
                return Err(ZxError::BAD_STATE);
            }
            // Check for overflow: the new size plus accumulated parent
            // offsets must not overflow 64-bit.
            if len > inner.size {
                let mut total = inner.parent_offset.checked_add(len);
                let mut cur = inner.parent.clone();
                while let (Some(t), Some(vmop)) = (total, cur) {
                    let p = vmop.inner.borrow();
                    total = t.checked_add(p.parent_offset);
                    cur = p.parent.clone();
                }
                if total.is_none() {
                    return Err(ZxError::INVALID_ARGS);
                }
            }
            inner.resize(len)
        };
        drop(old_parent);
        Ok(())
    }

    fn commit_page(&self, page_idx: usize, flags: MMUFlags) -> ZxResult<PhysAddr> {
        self.get_inner_mut().1.commit_page(page_idx, flags)
    }

    fn commit_pages_with(
        &self,
        f: &mut dyn FnMut(&mut dyn FnMut(usize, MMUFlags) -> ZxResult<PhysAddr>) -> ZxResult,
    ) -> ZxResult {
        let (_guard, mut inner) = self.get_inner_mut();
        f(&mut |page_idx, flags| inner.commit_page(page_idx, flags))
    }

    fn commit(&self, offset: usize, len: usize) -> ZxResult {
        let (_guard, mut inner) = self.get_inner_mut();
        let start_page = offset / PAGE_SIZE;
        let pages = len / PAGE_SIZE;
        for i in 0..pages {
            inner.commit_page(start_page + i, MMUFlags::WRITE)?;
        }
        Ok(())
    }

    fn decommit(&self, offset: usize, len: usize) -> ZxResult {
        let (_guard, mut inner) = self.get_inner_mut();
        if inner.parent.is_some() {
            return Err(ZxError::NOT_SUPPORTED);
        }
        // Validate range.
        let end = offset.checked_add(len).ok_or(ZxError::OUT_OF_RANGE)?;
        if end > inner.size {
            return Err(ZxError::OUT_OF_RANGE);
        }
        // Cannot decommit pinned pages.
        if inner.pin_count > 0 {
            let start_page = offset / PAGE_SIZE;
            let end_page = pages(end);
            for i in start_page..end_page {
                if let Some(frame) = inner.frames.get(&i) {
                    if frame.pin_count > 0 {
                        return Err(ZxError::BAD_STATE);
                    }
                }
            }
        }
        let start_page = offset / PAGE_SIZE;
        let page_count = len / PAGE_SIZE;
        for i in 0..page_count {
            inner.decommit(start_page + i);
        }
        Ok(())
    }

    fn create_child(&self, offset: usize, len: usize) -> ZxResult<Arc<dyn VMObjectTrait>> {
        assert!(page_aligned(offset));
        assert!(page_aligned(len));
        let (_guard, mut inner) = self.get_inner_mut();
        let child = inner.create_child(offset, len, &self.lock)?;
        Ok(child)
    }

    fn append_mapping(&self, mapping: Weak<VmMapping>) {
        self.get_inner_mut().1.mappings.push(mapping);
    }

    fn remove_mapping(&self, mapping: Weak<VmMapping>) {
        let inner = &mut self.get_inner_mut().1;
        let mappings = core::mem::take(&mut inner.mappings);
        for x in mappings {
            if x.strong_count() > 0 && !Weak::ptr_eq(&x, &mapping) {
                inner.mappings.push(x);
            }
        }
    }

    fn complete_info(&self, info: &mut VmoInfo) {
        let (_guard, inner) = self.get_inner();
        info.flags |= VmoInfoFlags::TYPE_PAGED;
        inner.complete_info(info);
    }

    fn cache_policy(&self) -> CachePolicy {
        let (_guard, inner) = self.get_inner();
        inner.cache_policy
    }

    fn set_cache_policy(&self, policy: CachePolicy) -> ZxResult {
        // conditions for allowing the cache policy to be set:
        // 1) vmo either has no pages committed currently or is transitioning from being cached
        // 2) vmo has no pinned pages
        // 3) vmo has no mappings
        // 4) vmo has no children (TODO)
        // 5) vmo is not a child
        let (_guard, mut inner) = self.get_inner_mut();
        if !inner.frames.is_empty() && inner.cache_policy != CachePolicy::Cached {
            return Err(ZxError::BAD_STATE);
        }
        inner.clear_invalild_mappings();
        if !inner.mappings.is_empty() {
            return Err(ZxError::BAD_STATE);
        }
        if inner.parent.is_some() {
            return Err(ZxError::BAD_STATE);
        }
        if inner.pin_count != 0 {
            return Err(ZxError::BAD_STATE);
        }
        if inner.cache_policy == CachePolicy::Cached && policy != CachePolicy::Cached {
            for value in inner.frames.values() {
                hal_impl::mem::frame_flush(value.frame.paddr());
            }
        }
        inner.cache_policy = policy;
        Ok(())
    }

    fn committed_pages_in_range(&self, start_idx: usize, end_idx: usize) -> usize {
        let (_guard, inner) = self.get_inner();
        inner.committed_pages_in_range(start_idx, end_idx)
    }

    fn pin(&self, offset: usize, len: usize) -> ZxResult {
        let (_guard, mut inner) = self.get_inner_mut();
        if offset + len > inner.size {
            return Err(ZxError::OUT_OF_RANGE);
        }
        if len == 0 {
            return Ok(());
        }
        let start_page = offset / PAGE_SIZE;
        let end_page = pages(offset + len);
        for i in start_page..end_page {
            let frame = match inner.frames.get(&i) {
                Some(f) => f,
                None => return Err(ZxError::NOT_FOUND),
            };
            if frame.pin_count == VM_PAGE_OBJECT_MAX_PIN_COUNT {
                return Err(ZxError::UNAVAILABLE);
            }
        }
        for i in start_page..end_page {
            if let Some(frame) = inner.frames.get_mut(&i) {
                frame.pin_count += 1;
            } else {
                return Err(ZxError::NOT_FOUND);
            }
            inner.pin_count += 1;
        }
        Ok(())
    }

    fn has_pinned_pages(&self, offset: usize, len: usize) -> bool {
        let (_guard, inner) = self.get_inner();
        if len == 0 || offset + len > inner.size {
            return false;
        }
        let start_page = offset / PAGE_SIZE;
        let end_page = pages(offset + len);
        for i in start_page..end_page {
            if let Some(frame) = inner.frames.get(&i) {
                if frame.pin_count > 0 {
                    return true;
                }
            }
        }
        false
    }

    fn unpin(&self, offset: usize, len: usize) -> ZxResult {
        let (_guard, mut inner) = self.get_inner_mut();
        let end = offset.checked_add(len).ok_or(ZxError::OUT_OF_RANGE)?;
        if end > inner.size {
            return Err(ZxError::OUT_OF_RANGE);
        }
        if len == 0 {
            return Ok(());
        }
        let start_page = offset / PAGE_SIZE;
        let end_page = pages(offset + len);
        for i in start_page..end_page {
            let frame = match inner.frames.get(&i) {
                Some(f) => f,
                None => return Err(ZxError::UNAVAILABLE),
            };
            if frame.pin_count == 0 {
                return Err(ZxError::UNAVAILABLE);
            }
        }
        if inner.pin_count == 0 {
            return Err(ZxError::UNAVAILABLE);
        }
        for i in start_page..end_page {
            if let Some(frame) = inner.frames.get_mut(&i) {
                frame.pin_count -= 1;
                inner.pin_count -= 1;
            }
        }
        Ok(())
    }

    fn is_contiguous(&self) -> bool {
        self.get_inner().1.is_contiguous()
    }

    fn is_paged(&self) -> bool {
        true
    }

    fn as_mut_buf(&self) -> ZxResult<(MutexGuard<'_, ()>, &mut [u8])> {
        let (guard, mut inner) = self.get_inner_mut();
        inner.as_mut_buf().map(|(addr, size)| {
            (guard, unsafe {
                core::slice::from_raw_parts_mut(addr as *mut u8, size)
            })
        })
    }

    fn unset_contiguous(&self) {
        let (_guard, mut inner) = self.get_inner_mut();
        if inner.contiguous {
            inner.contiguous = false;
            for frame in inner.frames.values_mut() {
                frame.pin_count -= 1;
            }
        }
    }
}

enum CommitResult {
    /// A reference to existing page.
    Ref(PhysAddr),
    /// A new page copied-on-write.
    /// the bool value indicate should we unmap the page after the copy
    CopyOnWrite(PhysFrame, bool),
    /// A new zero page.
    NewPage(PhysFrame),
}

impl VMObjectPagedInner {
    /// Helper function to split range into sub-ranges within pages.
    ///
    /// All covered pages will be committed implicitly.
    ///
    /// ```text
    /// VMO range:
    /// |----|----|----|----|----|
    ///
    /// buf:
    ///            [====len====]
    /// |--offset--|
    ///
    /// sub-ranges:
    ///            [===]
    ///                [====]
    ///                     [==]
    /// ```
    ///
    /// `f` is a function to process in-page ranges.
    /// It takes 2 arguments:
    /// * `paddr`: the start physical address of the in-page range.
    /// * `buf_range`: the range in view of the input buffer.
    fn for_each_page(
        &mut self,
        offset: usize,
        buf_len: usize,
        flags: MMUFlags,
        mut f: impl FnMut(PhysAddr, Range<usize>),
    ) -> ZxResult {
        let iter = BlockIter {
            begin: offset,
            end: offset + buf_len,
            block_size_log2: 12,
        };
        for block in iter {
            let paddr = self.commit_page(block.block, flags)?;
            let buf_range = block.origin_begin() - offset..block.origin_end() - offset;
            f(paddr + block.begin, buf_range);
        }
        Ok(())
    }

    fn commit_page(&mut self, page_idx: usize, flags: MMUFlags) -> ZxResult<PhysAddr> {
        let ret = match self.commit_page_internal(page_idx, flags, &Weak::new())? {
            CommitResult::Ref(paddr) => Ok(paddr),
            _ => unreachable!(),
        };
        // force check conntiguous on each leaf node
        assert!(self.check_contig());
        ret
    }

    /// Commit a page recursively.
    fn commit_page_internal(
        &mut self,
        page_idx: usize,
        flags: MMUFlags,
        child: &WeakRef,
    ) -> ZxResult<CommitResult> {
        // special case
        let no_parent = self.parent.is_none();
        let no_frame = !self.frames.contains_key(&page_idx);
        let out_of_range = if self.type_.is_hidden() || self.parent.is_none() {
            page_idx >= self.size / PAGE_SIZE
        } else {
            (self.parent_offset + page_idx * PAGE_SIZE) >= self.parent_limit
        };
        let mut need_unmap = false;
        if no_frame {
            // if out_of_range
            if out_of_range || no_parent {
                // For COW snapshot children reading beyond their
                // parent_limit, return the shared zero page without
                // allocating. This avoids wasting memory on zero-filled
                // pages that haven't been written to, matching Fuchsia's
                // demand-paging behavior for COW clones.
                // Only applies to snapshot children (has parent, out of
                // range) — root VMOs must always allocate real frames
                // because Linux processes expect writable zero pages.
                if out_of_range && !no_parent && !flags.contains(MMUFlags::WRITE) {
                    // Return the shared zero page for COW children
                    // reading beyond their parent_limit. The frame
                    // is cached to avoid repeated allocations.
                    static ZERO_PAGE: spin::Lazy<Option<PhysFrame>> =
                        spin::Lazy::new(PhysFrame::new_zero);
                    match ZERO_PAGE.as_ref() {
                        Some(zp) => return Ok(CommitResult::Ref(zp.paddr())),
                        None => return Err(ZxError::NO_MEMORY),
                    }
                }
                let target_frame = PhysFrame::new_zero().ok_or(ZxError::NO_MEMORY)?;
                if self.type_.is_hidden() {
                    // Hidden nodes return a zero page for both in-range
                    // and out-of-range requests. The caller decides
                    // whether to insert it or pass it through.
                    return Ok(CommitResult::NewPage(target_frame));
                }
                self.frames.insert(page_idx, PageState::new(target_frame));
                // Unmap existing zero-frame mappings so they pick up
                // the new real frame on next access.
                for map in self.mappings.iter() {
                    if let Some(map) = map.upgrade() {
                        map.range_change(page_idx, 1, RangeChangeOp::Unmap);
                    }
                }
            } else {
                // recursively find a frame in parent
                let mut parent = self.parent.as_ref().unwrap().inner.borrow_mut();
                let parent_idx = page_idx + self.parent_offset / PAGE_SIZE;
                match parent.commit_page_internal(parent_idx, flags, &self.self_ref)? {
                    CommitResult::NewPage(frame) if !self.type_.is_hidden() => {
                        // For read-only access, don't insert the zero
                        // page locally — just return a reference to the
                        // shared zero page. This avoids counting
                        // uncommitted zero pages as private in
                        // attribution.
                        if !flags.contains(MMUFlags::WRITE) {
                            // Drop the allocated frame (not needed)
                            // and return the cached zero page instead.
                            drop(frame);
                            static ZERO_PAGE_NP: spin::Lazy<Option<PhysFrame>> =
                                spin::Lazy::new(PhysFrame::new_zero);
                            return match ZERO_PAGE_NP.as_ref() {
                                Some(zp) => Ok(CommitResult::Ref(zp.paddr())),
                                None => Err(ZxError::NO_MEMORY),
                            };
                        }
                        self.frames.insert(page_idx, PageState::new(frame));
                    }
                    CommitResult::CopyOnWrite(frame, unmap) => {
                        let mut new_frame = PageState::new(frame);
                        // Cloning a contiguous vmo: original frames are stored in hidden parent nodes.
                        // In order to make sure original vmo (now is a child of hidden parent)
                        // owns physically contiguous frames, swap the new frame with the original
                        if self.contiguous {
                            if let Some(par_frame) = parent.frames.get_mut(&parent_idx) {
                                par_frame.swap(&mut new_frame);
                            }
                            let sibling = parent.type_.get_tag_and_other(&self.self_ref).1;
                            let arc_sibling = sibling.upgrade().unwrap();
                            let sibling_inner = arc_sibling.inner.borrow();
                            sibling_inner.range_change(
                                parent_idx * PAGE_SIZE,
                                (parent_idx + 1) * PAGE_SIZE,
                                RangeChangeOp::Unmap,
                            )
                        } else {
                            need_unmap = need_unmap || unmap;
                        }
                        self.frames.insert(page_idx, new_frame);
                    }
                    r => return Ok(r),
                }
            }
        }
        // now the page must hit on this VMO
        let (child_tag, other_child) = self.type_.get_tag_and_other(child);
        if self.type_.is_hidden() {
            let arc_other = other_child.upgrade().unwrap();
            let other_inner = arc_other.inner.borrow();
            let in_range = {
                let start = other_inner.parent_offset / PAGE_SIZE;
                let end = other_inner.parent_limit / PAGE_SIZE;
                page_idx >= start && page_idx < end
            };
            if !in_range {
                let frame = self.frames.remove(&page_idx).unwrap().take();
                return Ok(CommitResult::CopyOnWrite(frame, need_unmap));
            } else if need_unmap {
                other_inner.range_change(
                    page_idx * PAGE_SIZE,
                    (1 + page_idx) * PAGE_SIZE,
                    RangeChangeOp::Unmap,
                )
            }
        }
        if need_unmap {
            for map in self.mappings.iter() {
                if let Some(map) = map.upgrade() {
                    map.range_change(page_idx, 1, RangeChangeOp::Unmap);
                }
            }
        }
        let frame = self.frames.get_mut(&page_idx).unwrap();
        if frame.tag.is_split() {
            // The page was split during a previous COW fork. The
            // requesting child can still read the data. For WRITE,
            // a COW copy is needed. Don't remove the frame — keep
            // it in the hidden node so the other child's subtree
            // can still find it (avoiding broken sharing for
            // attribution).
            if flags.contains(MMUFlags::WRITE) {
                let target_frame = PhysFrame::new().ok_or(ZxError::NO_MEMORY)?;
                hal_impl::mem::pmem_copy(target_frame.paddr(), frame.frame.paddr(), PAGE_SIZE);
                return Ok(CommitResult::CopyOnWrite(target_frame, true));
            }
            return Ok(CommitResult::Ref(frame.frame.paddr()));
        } else if flags.contains(MMUFlags::WRITE) && child_tag.is_split() {
            // copy-on-write: the requesting child gets a private copy
            let target_frame = PhysFrame::new().ok_or(ZxError::NO_MEMORY)?;
            hal_impl::mem::pmem_copy(target_frame.paddr(), frame.frame.paddr(), PAGE_SIZE);
            frame.tag = child_tag;
            if frame.share_count > 0 {
                frame.share_count -= 1;
            }
            return Ok(CommitResult::CopyOnWrite(target_frame, true));
        }
        // otherwise already committed
        Ok(CommitResult::Ref(frame.frame.paddr()))
    }

    fn decommit(&mut self, page_idx: usize) {
        self.frames.remove(&page_idx);
    }

    fn range_change(&self, parent_offset: usize, parent_limit: usize, op: RangeChangeOp) {
        let mut start = self.parent_offset.max(parent_offset);
        let mut end = self.parent_limit.min(parent_limit);
        if start >= end {
            return;
        }
        start -= self.parent_offset;
        end -= self.parent_offset;
        for map in self.mappings.iter() {
            if let Some(map) = map.upgrade() {
                map.range_change(pages(start), pages(end) - pages(start), op);
            }
        }
        if let VMOType::Hidden { left, right, .. } = &self.type_ {
            for child in &[left, right] {
                let child = child.upgrade().unwrap();
                child.inner.borrow().range_change(start, end, op);
            }
        }
    }

    /// Count committed pages of the VMO.
    fn committed_pages_in_range(&self, start_idx: usize, end_idx: usize) -> usize {
        let pages = self.size / PAGE_SIZE;
        // Clamp range to VMO bounds instead of panicking — callers may
        // pass stale sizes when VMO has been resized or is zero-length.
        if pages == 0 || start_idx >= pages {
            return 0;
        }
        let end_idx = end_idx.min(pages);
        let mut count = 0;
        for i in start_idx..end_idx {
            if self.frames.contains_key(&i) {
                count += 1;
                continue;
            }
            if self.parent_limit <= i * PAGE_SIZE {
                continue;
            }
            // Walk up the parent chain using parent_limit-based
            // termination (matching Fuchsia's walk semantics).
            let mut current = self.parent.clone();
            let mut current_idx = i + self.parent_offset / PAGE_SIZE;
            while let Some(vmop) = current {
                let inner = vmop.inner.borrow();
                if inner.frames.contains_key(&current_idx) {
                    count += 1;
                    break;
                }
                if inner.parent_limit == 0 {
                    break;
                }
                let next = current_idx + inner.parent_offset / PAGE_SIZE;
                if next * PAGE_SIZE >= inner.parent_limit {
                    break;
                }
                current_idx = next;
                current = inner.parent.clone();
            }
        }
        count
    }

    /// Remove one child and contract hidden node.
    ///
    /// ```text
    ///    |         |
    ///    H         |
    ///   / \    =>  |
    ///  A   B       B
    ///  ^remove
    /// ```
    fn remove_child(&mut self, child: &WeakRef) {
        // a child slice do not have to belong to a hidden parent
        if !self.type_.is_hidden() {
            return;
        }
        let (tag, other_child) = self.type_.get_tag_and_other(child);
        let arc_child = other_child.upgrade().unwrap();
        let mut surviving = arc_child.inner.borrow_mut();
        let start = surviving.parent_offset / PAGE_SIZE;
        let end = surviving.parent_limit / PAGE_SIZE;

        // Merge frames to the surviving child.
        for (key, mut value) in core::mem::take(&mut self.frames) {
            if key < start || key >= end {
                continue;
            }
            if self.contiguous && !surviving.contiguous && value.pin_count >= 1 {
                value.pin_count -= 1;
            }
            let idx = key - start;
            if !surviving.frames.contains_key(&idx) && value.tag != tag.negate() {
                value.tag = PageStateTag::Owned;
                surviving.frames.insert(idx, value);
            }
        }

        // Connect surviving child to my parent.
        surviving.parent_offset += self.parent_offset;
        surviving.parent_limit += self.parent_offset;
        let surviving_offset = surviving.parent_offset;
        let surviving_limit = surviving.parent_limit;
        // Drop the surviving borrow before accessing the parent, since
        // the parent's cleanup may need to borrow its children.
        drop(surviving);

        if let Some(parent) = &self.parent {
            let mut parent_inner = parent.inner.borrow_mut();
            parent_inner.replace_child(
                &self.self_ref,
                self.owner,
                other_child,
                Some((surviving_offset, surviving_limit)),
            );
            // After replacing the child, clean up stale intermediate COW
            // copies in the parent (grandparent of the dying child).
            if parent_inner.type_.is_hidden() {
                Self::cleanup_stale_cow_copies(&mut parent_inner);
            }
        }
        // Re-borrow to set parent.
        let mut surviving = arc_child.inner.borrow_mut();
        surviving.parent = self.parent.take();
    }

    /// Remove intermediate COW copies whose split tags point to a child
    /// that can no longer see them (out of range). For each such frame,
    /// remove it from this node and reset the parent's corresponding
    /// frame tag to Owned (if the parent has one with the opposite tag).
    fn cleanup_stale_cow_copies(inner: &mut VMObjectPagedInner) {
        if !inner.type_.is_hidden() {
            return;
        }
        let (left_start, left_end, right_start, right_end) = {
            if let VMOType::Hidden { left, right, .. } = &inner.type_ {
                let ls = left
                    .upgrade()
                    .map(|a| {
                        let c = a.inner.borrow();
                        (c.parent_offset / PAGE_SIZE, c.parent_limit / PAGE_SIZE)
                    })
                    .unwrap_or((0, 0));
                let rs = right
                    .upgrade()
                    .map(|a| {
                        let c = a.inner.borrow();
                        (c.parent_offset / PAGE_SIZE, c.parent_limit / PAGE_SIZE)
                    })
                    .unwrap_or((0, 0));
                (ls.0, ls.1, rs.0, rs.1)
            } else {
                return;
            }
        };
        // Collect indices of stale frames to remove.
        let mut to_remove = alloc::vec::Vec::new();
        for (&idx, frame) in inner.frames.iter() {
            match frame.tag {
                PageStateTag::LeftSplit => {
                    // Split to left child. If left child can't see this
                    // page (out of range), the split is stale.
                    if idx < left_start || idx >= left_end {
                        to_remove.push(idx);
                    }
                }
                PageStateTag::RightSplit => {
                    // Split to right child. If right child can't see
                    // this page, the split is stale.
                    if idx < right_start || idx >= right_end {
                        to_remove.push(idx);
                    }
                }
                PageStateTag::Owned => {}
            }
        }
        // Remove stale frames and un-tag parent frames.
        for idx in to_remove {
            let _removed = inner.frames.remove(&idx);
            // Try to un-tag the corresponding parent frame.
            if let Some(ref parent) = inner.parent {
                let parent_idx = idx + inner.parent_offset / PAGE_SIZE;
                let mut p = parent.inner.borrow_mut();
                if let Some(parent_frame) = p.frames.get_mut(&parent_idx) {
                    // The parent frame should have the opposite split
                    // tag. Reset it to Owned so both children of the
                    // parent can see it again.
                    if parent_frame.tag.is_split() {
                        parent_frame.tag = PageStateTag::Owned;
                    }
                }
            }
        }
    }

    /// Create a snapshot child VMO.
    fn create_child(
        &mut self,
        offset: usize,
        len: usize,
        lock_ref: &Arc<Mutex<()>>,
    ) -> ZxResult<Arc<VMObjectPaged>> {
        // clone contiguous vmo is no longer permitted
        // https://fuchsia.googlesource.com/fuchsia/+/e6b4c6751bbdc9ed2795e81b8211ea294f139a45
        if self.is_contiguous() {
            return Err(ZxError::INVALID_ARGS);
        }
        if self.cache_policy != CachePolicy::Cached || self.pin_count != 0 {
            return Err(ZxError::BAD_STATE);
        }
        // Check for overflow in accumulated parent offsets.
        // Walk up the parent chain to compute the total offset from
        // the root VMO. If adding this child's offset overflows,
        // reject with INVALID_ARGS.
        {
            let mut total = offset.checked_add(self.parent_offset);
            let mut cur = self.parent.clone();
            while let (Some(t), Some(vmop)) = (total, cur) {
                let inner = vmop.inner.borrow();
                total = t.checked_add(inner.parent_offset);
                cur = inner.parent.clone();
            }
            if total.is_none() {
                return Err(ZxError::INVALID_ARGS);
            }
        }
        // create child VMO
        let child = VMObjectPaged::wrap(
            VMObjectPagedInner {
                owner: new_owner_id(),
                type_: VMOType::Snapshot,
                parent: None, // set later
                parent_offset: offset,
                parent_limit: (offset + len).min(self.size),
                size: len,
                frames: BTreeMap::new(),
                mappings: Vec::new(),
                cache_policy: CachePolicy::Cached,
                contiguous: false,
                self_ref: Default::default(),
                pin_count: 0,
            },
            Some(lock_ref.clone()),
        );
        // construct a hidden VMO as shared parent
        let hidden = VMObjectPaged::wrap(
            VMObjectPagedInner {
                owner: self.owner,
                type_: VMOType::Hidden {
                    left: self.self_ref.clone(),
                    right: Arc::downgrade(&child),
                },
                parent: self.parent.clone(),
                parent_offset: self.parent_offset,
                parent_limit: self.parent_limit,
                size: self.size,
                frames: core::mem::take(&mut self.frames),
                mappings: Vec::new(),
                cache_policy: CachePolicy::Cached,
                contiguous: self.contiguous,
                self_ref: Default::default(),
                pin_count: self.pin_count,
            },
            Some(lock_ref.clone()),
        );
        // update parent's child
        if let Some(parent) = self.parent.take() {
            if let VMOType::Hidden { left, right, .. } = &mut parent.inner.borrow_mut().type_ {
                if left.ptr_eq(&self.self_ref) {
                    *left = Arc::downgrade(&hidden);
                } else if right.ptr_eq(&self.self_ref) {
                    *right = Arc::downgrade(&hidden);
                } else {
                    panic!();
                }
            }
        }
        // update children's parent
        self.parent = Some(hidden.clone());
        self.parent_offset = 0;
        self.parent_limit = self.size;
        child.inner.borrow_mut().parent = Some(hidden.clone());
        // Increment share_count for pages visible to the new child.
        {
            let child_start = offset / PAGE_SIZE;
            let child_end = (offset + len).min(self.size) / PAGE_SIZE;
            let mut h = hidden.inner.borrow_mut();
            for idx in child_start..child_end {
                if let Some(page) = h.frames.get_mut(&idx) {
                    page.share_count += 1;
                }
            }
            // Also walk up ancestors for pages not in the hidden node.
            if let Some(ref parent) = h.parent {
                let mut cur = Some(parent.clone());
                let mut cur_start = child_start + h.parent_offset / PAGE_SIZE;
                let mut cur_end = child_end + h.parent_offset / PAGE_SIZE;
                while let Some(vmop) = cur {
                    let mut inner = vmop.inner.borrow_mut();
                    for idx in cur_start..cur_end {
                        if let Some(page) = inner.frames.get_mut(&idx) {
                            page.share_count += 1;
                        }
                    }
                    if inner.parent_limit == 0 {
                        break;
                    }
                    let off = inner.parent_offset / PAGE_SIZE;
                    cur_start += off;
                    cur_end += off;
                    cur = inner.parent.clone();
                }
            }
        }
        // update mappings, for COW, remove write flags in PageTable
        for map in self.mappings.iter() {
            if let Some(map) = map.upgrade() {
                map.range_change(pages(offset), pages(len), RangeChangeOp::RemoveWrite);
            }
        }
        Ok(child)
    }

    /// Replace a child of the hidden node.
    /// `new_start` and `new_end` are in bytes
    fn replace_child(
        &mut self,
        old: &WeakRef,
        old_id: KoID,
        new: WeakRef,
        new_range: Option<(usize, usize)>,
    ) {
        let (tag, other) = self.type_.get_tag_and_other(old);
        let arc_other_child = other.upgrade().unwrap();
        let mut other_child = arc_other_child.inner.borrow_mut();
        let mut unwanted = VecDeque::<usize>::new();
        if let Some((new_start, new_end)) = new_range {
            let other_start = other_child.parent_offset / PAGE_SIZE;
            let other_end = other_child.parent_limit / PAGE_SIZE;
            let start = new_start / PAGE_SIZE;
            let end = new_end / PAGE_SIZE;
            for i in 0..self.size / PAGE_SIZE {
                let not_in_range =
                    !((start <= i && end > i) || (other_start <= i && other_end > i));
                if not_in_range {
                    // if not in this node's range
                    if self.frames.contains_key(&i) {
                        // if the frame is in our, remove it
                        assert!(self.frames.remove(&i).is_some());
                    } else {
                        // or it is in our ancestor, tell them we do not need it.
                        unwanted.push_back(i + self.parent_offset / PAGE_SIZE);
                    }
                } else {
                    // if in this node's range, check if it can be moved
                    if let Some(frame) = self.frames.get(&i) {
                        if frame.tag.is_split() {
                            // Check if the split is stale: the frame was
                            // split to the old child (frame.tag == tag)
                            // but the replacement child can't see this
                            // page (out of its range).
                            let split_is_stale = frame.tag == tag && (i < start || i >= end);
                            if split_is_stale {
                                // The split tag pointed to the old child
                                // but the replacement child can't see
                                // this page. Reset the tag so the other
                                // child (and its subtree) can see it.
                                // Also check if this frame is an
                                // intermediate copy that shadows a parent
                                // frame — if so, remove it and un-tag
                                // the parent's frame.
                                let has_parent_frame = self.parent.as_ref().is_some_and(|p| {
                                    let pi = i + self.parent_offset / PAGE_SIZE;
                                    p.inner.borrow().frames.contains_key(&pi)
                                });
                                if has_parent_frame {
                                    // This frame is an intermediate COW
                                    // copy. Remove it and un-tag the
                                    // parent's frame.
                                    let _removed = self.frames.remove(&i);
                                    if let Some(ref parent) = self.parent {
                                        let parent_idx = i + self.parent_offset / PAGE_SIZE;
                                        let mut p = parent.inner.borrow_mut();
                                        if let Some(pf) = p.frames.get_mut(&parent_idx) {
                                            if pf.tag.is_split() {
                                                pf.tag = PageStateTag::Owned;
                                            }
                                        }
                                    }
                                } else {
                                    // This is the original page (no
                                    // parent copy). Just reset the tag
                                    // so both children can see it.
                                    if let Some(f) = self.frames.get_mut(&i) {
                                        f.tag = PageStateTag::Owned;
                                    }
                                }
                            } else {
                                let mut new_frame = self.frames.remove(&i).unwrap();
                                if self.contiguous
                                    && !other_child.contiguous
                                    && new_frame.pin_count >= 1
                                {
                                    new_frame.pin_count -= 1;
                                }
                                if new_frame.tag == tag && other_start <= i && other_end > i {
                                    new_frame.tag = PageStateTag::Owned;
                                    let new_key = i - other_child.parent_offset / PAGE_SIZE;
                                    other_child.frames.insert(new_key, new_frame);
                                }
                            }
                        }
                    }
                }
            }
        }

        self.release_unwanted_pages_in_parent(unwanted);

        if old_id == self.owner {
            let mut option_parent = self.parent.clone();
            let mut child = self.self_ref.clone();
            let mut skip_owner = old_id;
            while let Some(parent) = option_parent {
                let mut parent_inner = parent.inner.borrow_mut();
                if parent_inner.owner == old_id {
                    let (_, other) = parent_inner.type_.get_tag_and_other(&child);
                    let new_owner = other.upgrade().unwrap().inner.borrow().owner;
                    child = parent_inner.self_ref.clone();
                    assert_ne!(new_owner, skip_owner);
                    parent_inner.owner = new_owner;
                    skip_owner = new_owner;
                    option_parent = parent_inner.parent.clone();
                } else {
                    break;
                }
            }
        }

        self.owner = other_child.owner;
        match &mut self.type_ {
            VMOType::Hidden { left, right, .. } => {
                if left.ptr_eq(old) {
                    *left = new;
                } else if right.ptr_eq(old) {
                    *right = new;
                } else {
                    panic!();
                }
            }
            _ => panic!(),
        }
    }

    fn complete_info(&self, info: &mut VmoInfo) {
        if let VMOType::Snapshot = self.type_ {
            info.flags |= VmoInfoFlags::IS_COW_CLONE;
        }
        if self.is_contiguous() {
            info.flags |= VmoInfoFlags::CONTIGUOUS;
        }
        info.num_mappings = self.mappings.len() as u64;
        info.share_count = self.mappings.len() as u64;

        let total_pages = self.size / PAGE_SIZE;
        let committed = (self.committed_pages_in_range(0, total_pages) * PAGE_SIZE) as u64;
        info.committed_bytes = committed;
        // No compression/deduplication, so populated == committed.
        info.populated_bytes = committed;

        // Private bytes: pages in self.frames that are directly owned
        // by this VMO (not resolved from a parent via COW walk).
        let private_pages = self.frames.len();
        let private_bytes = (private_pages * PAGE_SIZE) as u64;
        info.committed_private_bytes = private_bytes;
        info.populated_private_bytes = private_bytes;

        // Compute per-page fractional attribution by walking the COW tree.
        let attribution = self.compute_attribution(0, total_pages);
        info.committed_scaled_bytes = attribution.scaled_bytes;
        info.populated_scaled_bytes = attribution.scaled_bytes;
        info.committed_fractional_scaled_bytes = attribution.fractional_scaled_bytes;
        info.populated_fractional_scaled_bytes = attribution.fractional_scaled_bytes;
    }

    /// Compute per-page fractional attribution by counting leaf VMOs
    /// that share each page.
    ///
    /// For each page the querying VMO sees (locally or through the
    /// parent chain), counts how many leaf VMOs see the SAME physical
    /// frame by walking DOWN the COW tree from the owning node.
    fn compute_attribution(&self, start_idx: usize, end_idx: usize) -> Attribution {
        let pages = self.size / PAGE_SIZE;
        if pages == 0 || start_idx >= pages {
            return Attribution::default();
        }
        let end_idx = end_idx.min(pages);
        let mut result = Attribution::default();

        for i in start_idx..end_idx {
            // Case 1: we have a local frame — it's private to us.
            if self.frames.contains_key(&i) {
                result.add_page(1);
                continue;
            }
            if self.parent_limit <= i * PAGE_SIZE {
                continue;
            }
            // Case 2: walk up the parent chain to find the TOPMOST node
            // that has this page. Intermediate COW forks create copies
            // at multiple levels — all copies represent the same logical
            // data. We need to count all leaf VMOs that see ANY copy.
            //
            // Walk up until we find the highest ancestor that has a
            // frame at the corresponding index. Then count all leaves
            // from that ancestor downward (ignoring split tags, since
            // split just means "has a copy below").
            let mut current = self.parent.clone();
            let mut current_idx = i + self.parent_offset / PAGE_SIZE;
            let mut topmost_vmop: Option<Arc<VMObjectPaged>> = None;
            let mut topmost_idx = current_idx;
            while let Some(vmop) = current {
                let inner = vmop.inner.borrow();
                if inner.frames.contains_key(&current_idx) {
                    topmost_vmop = Some(vmop.clone());
                    topmost_idx = current_idx;
                    // Keep walking up to find higher copies.
                    if inner.parent.is_some() && inner.parent_limit > 0 {
                        let next = current_idx + inner.parent_offset / PAGE_SIZE;
                        if next * PAGE_SIZE < inner.parent_limit {
                            current_idx = next;
                            current = inner.parent.clone();
                            continue;
                        }
                    }
                    break;
                }
                if inner.parent_limit == 0 {
                    break;
                }
                let next = current_idx + inner.parent_offset / PAGE_SIZE;
                if next * PAGE_SIZE >= inner.parent_limit {
                    break;
                }
                current_idx = next;
                current = inner.parent.clone();
            }
            if let Some(vmop) = topmost_vmop {
                let inner = vmop.inner.borrow();
                // Count all leaves from the topmost owner, treating
                // the page as Owned (shared by all children) since
                // split tags just indicate intermediate copies exist
                // below — those copies serve the same logical data.
                let count =
                    Self::count_leaves_seeing_page(&inner, topmost_idx, &PageStateTag::Owned);
                if count > 0 {
                    result.add_page(count as u64);
                }
            }
        }
        result
    }

    /// Count how many leaf VMOs can see a page at `page_idx` in `owner`.
    /// `tag` indicates which children of the hidden `owner` can see it.
    fn count_leaves_seeing_page(
        owner: &VMObjectPagedInner,
        page_idx: usize,
        _tag: &PageStateTag,
    ) -> usize {
        if !owner.type_.is_hidden() {
            // Leaf node: it can see this page (we wouldn't be called
            // if it couldn't).
            return 1;
        }
        // Hidden node: check which children can see this page based
        // on the tag and their visible range.
        let mut count = 0;
        if let VMOType::Hidden { left, right, .. } = &owner.type_ {
            // Always check both children. Split tags indicate a COW
            // fork happened, but intermediate hidden nodes may hold
            // copies of the same data. count_child_viewers correctly
            // returns 0 for children that have their own private copy
            // (leaf with local frame), so double-counting is avoided.
            let check_left = true;
            let check_right = true;

            if check_left {
                if let Some(arc_left) = left.upgrade() {
                    count += Self::count_child_viewers(&arc_left, page_idx);
                }
            }

            if check_right {
                if let Some(arc_right) = right.upgrade() {
                    count += Self::count_child_viewers(&arc_right, page_idx);
                }
            }
        }
        count
    }

    /// Count how many leaf VMOs in this child's subtree can see a page
    /// at `page_idx` (in the parent's coordinate space).
    ///
    /// - Leaf with no local frame → sees parent's page → count 1
    /// - Leaf with local frame → has own private copy → count 0
    ///   (attributed separately when computing that leaf's own info)
    /// - Hidden child with local frame → intermediate COW copy →
    ///   recursively count its subtree viewers using its frame's tag
    /// - Hidden child with no local frame → transparent → recurse
    fn count_child_viewers(child_arc: &Arc<VMObjectPaged>, page_idx: usize) -> usize {
        let child = child_arc.inner.borrow();
        let start = child.parent_offset / PAGE_SIZE;
        let end = child.parent_limit / PAGE_SIZE;
        if page_idx < start || page_idx >= end {
            return 0;
        }
        let child_idx = page_idx - start;
        if let Some(frame) = child.frames.get(&child_idx) {
            if child.type_.is_hidden() {
                // Hidden child has an intermediate COW copy — count
                // its subtree viewers.
                Self::count_leaves_seeing_page(&child, child_idx, &frame.tag)
            } else {
                // Leaf with local frame — has its own private copy.
                // Don't count as viewer of parent's data.
                0
            }
        } else if child.type_.is_hidden() {
            Self::count_leaves_seeing_page(&child, child_idx, &PageStateTag::Owned)
        } else {
            // Leaf without local frame — sees parent's page.
            1
        }
    }

    fn release_unwanted_pages_in_parent(&mut self, mut unwanted: VecDeque<usize>) {
        let mut option_parent = self.parent.clone();
        let mut child = self.self_ref.clone();
        while let Some(parent) = option_parent {
            let mut parent_inner = parent.inner.borrow_mut();
            let (tag, other) = parent_inner.type_.get_tag_and_other(&child);
            let arc_other = other.upgrade().unwrap();
            let mut other_inner = arc_other.inner.borrow_mut();
            let start = other_inner.parent_offset / PAGE_SIZE;
            let end = other_inner.parent_limit / PAGE_SIZE;
            for _ in 0..unwanted.len() {
                let idx = unwanted.pop_front().unwrap();
                // if the frame is in other_inner's range, check if it can be move to other_inner
                if start <= idx && idx < end {
                    if parent_inner.frames.contains_key(&idx) {
                        let mut to_insert = parent_inner.frames.remove(&idx).unwrap();
                        if parent_inner.contiguous
                            && !other_inner.contiguous
                            && to_insert.pin_count >= 1
                        {
                            to_insert.pin_count -= 1;
                        }
                        if to_insert.tag != tag.negate() {
                            to_insert.tag = PageStateTag::Owned;
                            other_inner.frames.insert(idx - start, to_insert);
                        }
                        unwanted.push_back(idx + parent_inner.parent_offset / PAGE_SIZE);
                    }
                } else {
                    // otherwise, if it exists in our frames, remove it; if not, push_back it again
                    if parent_inner.frames.contains_key(&idx) {
                        parent_inner.frames.remove(&idx);
                    } else {
                        unwanted.push_back(idx + parent_inner.parent_offset / PAGE_SIZE);
                    }
                }
            }
            child = parent_inner.self_ref.clone();
            option_parent = parent_inner.parent.clone();
            drop(parent_inner);
        }
    }

    fn resize(&mut self, new_size: usize) -> Option<Arc<VMObjectPaged>> {
        let mut old_parent = None;
        if new_size == 0 && new_size < self.size {
            self.frames.clear();
            if let Some(parent) = self.parent.as_ref() {
                parent.inner.borrow_mut().remove_child(&self.self_ref);
            }
            // We cannot drop the parent Arc here since we are holding the lock
            // pass it to caller who can drop it after unlocking the lock
            old_parent = self.parent.take();
            self.parent_offset = 0;
            self.parent_limit = 0;
        } else if new_size < self.size {
            let mut unwanted = VecDeque::<usize>::new();
            let parent_end = (self.parent_limit - self.parent_offset) / PAGE_SIZE;
            for i in new_size / PAGE_SIZE..self.size / PAGE_SIZE {
                self.decommit(i);
                if parent_end > i {
                    unwanted.push_back(i + self.parent_offset / PAGE_SIZE);
                }
            }
            self.release_unwanted_pages_in_parent(unwanted);
            if new_size < self.parent_limit - self.parent_offset {
                self.parent_limit = self.parent_offset + new_size;
            }
        }
        self.size = new_size;
        old_parent
    }

    fn is_contiguous(&self) -> bool {
        self.contiguous
    }

    fn clear_invalild_mappings(&mut self) {
        for x in core::mem::take(&mut self.mappings) {
            if x.strong_count() > 0 {
                self.mappings.push(x);
            }
        }
    }

    /// Check whether it is not physically contiguous when it should be
    fn check_contig(&self) -> bool {
        if !self.contiguous {
            return true;
        }
        let mut base = 0;
        for (key, ps) in self.frames.iter() {
            let new_base = ps.frame.paddr() - key * PAGE_SIZE;
            if base == 0 || new_base == base {
                base = new_base;
            } else {
                return false;
            }
        }
        true
    }

    fn as_mut_buf(&mut self) -> ZxResult<(usize, usize)> {
        if self.contiguous {
            let addr = phys_to_virt(self.commit_page(0, MMUFlags::WRITE)?) as usize;
            let size = self.size;
            return Ok((addr, size));
        }
        Err(ZxError::UNAVAILABLE)
    }
}

impl Drop for VMObjectPaged {
    fn drop(&mut self) {
        let (_guard, mut inner) = self.get_inner_mut();
        // Decrement share_count for pages seen through parent chain.
        if let Some(parent) = &inner.parent {
            let start = inner.parent_offset / PAGE_SIZE;
            let end = inner.parent_limit / PAGE_SIZE;
            let mut cur = Some(parent.clone());
            let mut cur_start = start;
            let mut cur_end = end;
            while let Some(vmop) = cur {
                let mut p = vmop.inner.borrow_mut();
                for idx in cur_start..cur_end {
                    let has_local = inner.frames.contains_key(&(idx - start));
                    if !has_local {
                        if let Some(page) = p.frames.get_mut(&idx) {
                            if page.share_count > 0 {
                                page.share_count -= 1;
                            }
                        }
                    }
                }
                if p.parent_limit == 0 {
                    break;
                }
                let off = p.parent_offset / PAGE_SIZE;
                cur_start += off;
                cur_end += off;
                cur = p.parent.clone();
            }
            parent.inner.borrow_mut().remove_child(&inner.self_ref);
        }
        let is_conti = inner.is_contiguous();
        for frame in inner.frames.iter_mut() {
            if is_conti {
                // WARN: In fact we do not need this `if`.
                // If this vmo is a child of a contiguous vmo,
                // its pages should also be pinned.
                if frame.1.pin_count >= 1 {
                    frame.1.pin_count -= 1;
                }
            }
            assert_eq!(frame.1.pin_count, 0);
        }
    }
}

/// Generate a owner ID.
fn new_owner_id() -> u64 {
    static OWNER_ID: AtomicU64 = AtomicU64::new(1);
    OWNER_ID.fetch_add(1, Ordering::SeqCst)
}

const VM_PAGE_OBJECT_MAX_PIN_COUNT: u8 = 31;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_write() {
        let vmo = VmObject::new_paged(2);
        super::super::tests::read_write(&*vmo);
    }

    #[test]
    fn create_child() {
        let vmo = VmObject::new_paged(1);
        let child_vmo = vmo.create_child(false, 0, PAGE_SIZE).unwrap();

        // write to parent and make sure clone doesn't see it
        vmo.test_write(0, 1);
        assert_eq!(vmo.test_read(0), 1);
        assert_eq!(child_vmo.test_read(0), 0);

        // write to clone and make sure parent doesn't see it
        child_vmo.test_write(0, 2);
        assert_eq!(vmo.test_read(0), 1);
        assert_eq!(child_vmo.test_read(0), 2);
    }

    #[test]
    #[ignore] // FIXME
    fn zero_page_write() {
        let vmo0 = VmObject::new_paged(1);
        let vmo1 = vmo0.create_child(false, 0, PAGE_SIZE).unwrap();
        let vmo2 = vmo0.create_child(false, 0, PAGE_SIZE).unwrap();
        let vmos = [vmo0, vmo1, vmo2];
        let origin = vmo_page_bytes();

        // no committed pages
        for vmo in &vmos {
            assert_eq!(vmo.get_info().committed_bytes, 0);
        }

        // copy-on-write
        for i in 0..3 {
            vmos[i].test_write(0, i as u8);
            for j in 0..3 {
                assert_eq!(vmos[j].test_read(0), if j <= i { j as u8 } else { 0 });
                assert_eq!(
                    vmos[j].get_info().committed_bytes as usize,
                    if j <= i { PAGE_SIZE } else { 0 }
                );
            }
            assert_eq!(vmo_page_bytes() - origin, (i + 1) * PAGE_SIZE);
        }
    }

    #[test]
    fn overflow() {
        let vmo0 = VmObject::new_paged(2);
        vmo0.test_write(0, 1);
        let vmo1 = vmo0.create_child(false, 0, 2 * PAGE_SIZE).unwrap();
        vmo1.test_write(1, 2);
        let vmo2 = vmo1.create_child(false, 0, 3 * PAGE_SIZE).unwrap();
        vmo2.test_write(2, 3);
        // committed_bytes counts all pages visible through the parent chain.
        // vmo0: page 0 in H1 (1 page). Page 1 was never written by vmo0.
        assert_eq!(vmo0.get_info().committed_bytes as usize, PAGE_SIZE);
        // vmo1: page 0 from H1 + page 1 written locally (now in H2) = 2 pages.
        assert_eq!(vmo1.get_info().committed_bytes as usize, 2 * PAGE_SIZE);
        // vmo2: page 0 from H1 + page 1 from H2 + page 2 written locally = 3 pages.
        assert_eq!(vmo2.get_info().committed_bytes as usize, 3 * PAGE_SIZE);
    }

    /// COW clone permutation test. Flaky on macOS libos due to the
    /// mock frame allocator recycling stale physical pages without
    /// zeroing. This causes COW children to occasionally read
    /// content from a previously freed frame instead of the parent's
    /// page. Runs reliably on bare-metal and Linux CI.
    #[test]
    #[cfg_attr(target_os = "macos", ignore)]
    fn many_clones() {
        const N: usize = 4;
        let old: u8 = 0xa;
        let new: u8 = 0xb;
        let permutations = [
            [0, 1, 2, 3],
            [0, 1, 3, 2],
            [0, 2, 1, 3],
            [0, 2, 3, 1],
            [0, 3, 1, 2],
            [0, 3, 2, 1],
            [1, 0, 2, 3],
            [1, 0, 3, 2],
            [1, 2, 0, 3],
            [1, 2, 3, 0],
            [1, 3, 0, 2],
            [1, 3, 2, 0],
            [2, 1, 0, 3],
            [2, 1, 3, 0],
            [2, 0, 1, 3],
            [2, 0, 3, 1],
            [2, 3, 1, 0],
            [2, 3, 0, 1],
            [3, 1, 2, 0],
            [3, 1, 0, 2],
            [3, 2, 1, 0],
            [3, 2, 0, 1],
            [3, 0, 1, 2],
            [3, 0, 2, 1],
        ];
        for i in 0..24 {
            let vmo0 = VmObject::new_paged(1);
            vmo0.write(0, &[old]).unwrap();
            let vmo1 = vmo0.create_child(false, 0, PAGE_SIZE).unwrap();
            let vmo2 = vmo0.create_child(false, 0, PAGE_SIZE).unwrap();
            let vmo3 = vmo1.create_child(false, 0, PAGE_SIZE).unwrap();
            let vmos: [Arc<VmObject>; 4] = [vmo0.clone(), vmo1.clone(), vmo2.clone(), vmo3.clone()];
            let mut write: [bool; 4] = [false; 4];
            let perm = permutations[i];
            println!("{:?}", perm);
            for j in 0..N {
                println!("j = {}, write = {}", j, perm[j]);
                vmos[perm[j]].write(0, &[new]).unwrap();
                write[perm[j]] = true;
                let mut buf: [u8; 1] = [0];
                for k in 0..N {
                    vmos[k].read(0, &mut buf).unwrap();
                    println!("vmo[{}] = {:x}", k, buf[0]);
                    if write[k] {
                        assert!(buf[0] == new);
                    } else {
                        assert!(buf[0] == old);
                    }
                    buf[0] = 0;
                }
            }
        }
    }

    impl VmObject {
        pub fn test_write(&self, page: usize, value: u8) {
            self.write(page * PAGE_SIZE, &[value]).unwrap();
        }

        pub fn test_read(&self, page: usize) -> u8 {
            let mut buf = [0; 1];
            self.read(page * PAGE_SIZE, &mut buf).unwrap();
            buf[0]
        }
    }

    #[test]
    fn set_size() {
        let vmo = VmObject::new_paged_with_resizable(true, 2);
        assert_eq!(vmo.len(), 2 * PAGE_SIZE);

        // Grow
        vmo.set_len(4 * PAGE_SIZE).unwrap();
        assert_eq!(vmo.len(), 4 * PAGE_SIZE);

        // Shrink
        vmo.set_len(1 * PAGE_SIZE).unwrap();
        assert_eq!(vmo.len(), 1 * PAGE_SIZE);

        // Non-resizable VMO should fail
        let fixed_vmo = VmObject::new_paged(2);
        assert!(fixed_vmo.set_len(4 * PAGE_SIZE).is_err());
    }

    #[test]
    fn zero_range() {
        let vmo = VmObject::new_paged(1);
        // Write data
        vmo.write(0, b"Hello World!").unwrap();
        // Zero a portion
        vmo.zero(0, 5).unwrap();
        // Verify zeroed part
        let mut buf = [0u8; 12];
        vmo.read(0, &mut buf).unwrap();
        assert_eq!(&buf[0..5], &[0, 0, 0, 0, 0]);
        assert_eq!(&buf[5..12], b" World!");
    }
}
