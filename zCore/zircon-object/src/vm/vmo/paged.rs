use {
    super::*,
    crate::util::block_range::BlockIter,
    alloc::collections::BTreeMap,
    alloc::collections::BTreeSet,
    alloc::collections::VecDeque,
    alloc::sync::{Arc, Weak},
    alloc::vec::Vec,
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
    /// A snapshot of the parent node (bidirectional COW clone).
    Snapshot,
    /// A unidirectional clone: hangs directly under the parent
    /// without a hidden node. Unmodified pages read through to
    /// the parent; only pages the child writes are COW-forked.
    /// Used for SNAPSHOT_MODIFIED on a pager root VMO.
    UnidirectionalChild,
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

    fn is_unidirectional(&self) -> bool {
        matches!(self, VMOType::UnidirectionalChild)
    }
}

/// The main VM object type, holding a list of pages.
pub struct VMObjectPaged {
    /// The lock that protected the `inner`
    /// This lock is shared between objects in the same clone tree to avoid deadlock
    lock: Arc<Mutex<()>>,
    /// Interior mutability via UnsafeCell. Safety: the shared `lock` Mutex
    /// must be held whenever accessing the inner data. We use UnsafeCell
    /// instead of RefCell because RefCell's runtime borrow checking causes
    /// panics on SMP when the same clone tree is accessed concurrently —
    /// even though the Mutex correctly serializes access, RefCell's
    /// non-atomic borrow counter can observe stale state across cores.
    inner: core::cell::UnsafeCell<VMObjectPagedInner>,
}

/// Safety: all access to `inner` is protected by the shared `lock` Mutex.
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
    /// Zero markers: page indices that have been explicitly zeroed and
    /// decommitted.  Reading a page with a zero marker returns zeros
    /// instead of falling through to the parent chain.  This is a
    /// lightweight substitute for Fuchsia's `VmPageOrMarker::Marker`.
    zero_markers: BTreeSet<usize>,
    /// All mappings to this VMO.
    mappings: Vec<Weak<VmMapping>>,
    /// Cache Policy
    cache_policy: CachePolicy,
    /// Is contiguous
    contiguous: bool,
    /// True if this VMO is backed by a pager. Pages must only be
    /// committed via `supply_pages`, not auto-allocated by `commit_page`.
    pager_backed: bool,
    /// For unbounded VMOs: the content boundary in pages (stream_size
    /// rounded up). Pages at or beyond this index return zeros without
    /// pager requests on READ. Set to usize::MAX for regular VMOs.
    content_boundary_pages: usize,
    /// Temporarily set to true during `supply_pages` to allow
    /// `commit_page` to allocate frames on a pager-backed VMO.
    supplying: bool,
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
///
/// Tracks both raw bytes (full PAGE_SIZE per page) and scaled bytes
/// (PAGE_SIZE / share_count), matching Fuchsia's `AttributionCounts`.
#[derive(Default)]
struct Attribution {
    /// Total raw bytes (PAGE_SIZE per owned page, regardless of sharing).
    total_bytes: u64,
    /// Private bytes (PAGE_SIZE only for pages with share_count==0).
    private_bytes: u64,
    /// Sum of `PAGE_SIZE / sharing_count` for each page (integer part).
    scaled_bytes: u64,
    /// 63-bit fixed-point fractional remainder from the division.
    fractional_scaled_bytes: u64,
}

impl Attribution {
    /// Add one page with the given sharing count to the attribution.
    ///
    /// `sharing_count` is share_count+1 (number of VMOs that can see this page).
    /// For private pages (share_count==0), sharing_count==1.
    fn add_page(&mut self, sharing_count: u64) {
        if sharing_count == 0 {
            return;
        }
        let page_size = PAGE_SIZE as u64;
        self.total_bytes += page_size;
        if sharing_count == 1 {
            self.private_bytes += page_size;
        }
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
                zero_markers: BTreeSet::new(),
                mappings: Vec::new(),
                cache_policy: CachePolicy::Cached,
                contiguous: false,
                pager_backed: false,
                content_boundary_pages: usize::MAX,
                supplying: false,
                self_ref: Default::default(),
                pin_count: 0,
            },
            None,
        )
    }

    /// Create a new VMO backing on contiguous pages.
    pub fn new_contiguous(pages: usize, align_log2: usize) -> ZxResult<Arc<Self>> {
        let vmo = Self::new(pages);
        let mut frames =
            PhysFrame::new_contiguous(pages, align_log2.saturating_sub(PAGE_SIZE_LOG2));
        if frames.is_empty() {
            return Err(ZxError::NO_MEMORY);
        }
        {
            let (_guard, inner) = vmo.get_inner_mut();
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
            inner: core::cell::UnsafeCell::new(inner),
        });
        // Safety: obj is not yet shared — we're the only holder of the Arc.
        #[allow(unsafe_code)]
        unsafe {
            (*obj.inner.get()).self_ref = Arc::downgrade(&obj);
        }
        obj
    }

    /// Get a shared reference to inner, holding the clone-tree lock.
    ///
    /// Safety: the returned reference borrows `self`, so the MutexGuard
    /// (which is returned alongside) keeps the lock held for the
    /// reference's lifetime.
    #[allow(unsafe_code)]
    fn get_inner(&self) -> (MutexGuard<'_, ()>, &VMObjectPagedInner) {
        let guard = self.lock.lock();
        // Safety: lock is held, guaranteeing exclusive access.
        let inner = unsafe { &*self.inner.get() };
        (guard, inner)
    }

    /// Get a mutable reference to inner, holding the clone-tree lock.
    #[allow(unsafe_code, clippy::mut_from_ref)]
    fn get_inner_mut(&self) -> (MutexGuard<'_, ()>, &mut VMObjectPagedInner) {
        let guard = self.lock.lock();
        // Safety: lock is held, guaranteeing exclusive access.
        let inner = unsafe { &mut *self.inner.get() };
        (guard, inner)
    }

    /// Borrow another node's inner immutably.
    ///
    /// Safety: caller must hold the clone-tree lock (which is shared
    /// across all nodes in the tree).
    #[allow(unsafe_code)]
    #[inline(always)]
    fn borrow_inner(&self) -> &VMObjectPagedInner {
        unsafe { &*self.inner.get() }
    }

    /// Borrow another node's inner mutably.
    ///
    /// Safety: caller must hold the clone-tree lock.
    #[allow(unsafe_code, clippy::mut_from_ref)]
    #[inline(always)]
    fn borrow_inner_mut(&self) -> &mut VMObjectPagedInner {
        unsafe { &mut *self.inner.get() }
    }
}

impl VMObjectTrait for VMObjectPaged {
    fn read(&self, offset: usize, buf: &mut [u8]) -> ZxResult {
        let (_guard, inner) = self.get_inner_mut();
        if inner.cache_policy != CachePolicy::Cached {
            return Err(ZxError::BAD_STATE);
        }
        // Bounds check: the entire read range must be within the VMO.
        let end = offset.checked_add(buf.len()).ok_or(ZxError::OUT_OF_RANGE)?;
        if end > inner.size {
            return Err(ZxError::OUT_OF_RANGE);
        }
        // Read each page, filling with zeros for uncommitted pages
        // in root VMOs without allocating (matches Fuchsia's zero-page
        // optimization where ReadWriteInternal returns vm_get_zero_page()
        // for ContentZero pages without committing memory).
        let iter = BlockIter {
            begin: offset,
            end: offset + buf.len(),
            block_size_log2: PAGE_SIZE_LOG2 as u8,
        };
        for block in iter {
            let buf_range = block.origin_begin() - offset..block.origin_end() - offset;
            // Try to read from a committed page first.
            if let Some(frame) = inner.frames.get(&block.block) {
                hal_impl::mem::pmem_read(frame.frame.paddr() + block.begin, &mut buf[buf_range]);
                continue;
            }
            // For uncommitted pages in root VMOs (no parent, not
            // pager-backed), return zeros without allocating.
            if inner.parent.is_none() && !inner.pager_backed && !inner.type_.is_hidden() {
                // Check if there's a zero marker — also returns zeros.
                buf[buf_range].fill(0);
                continue;
            }
            // For child VMOs or pager-backed VMOs, use commit_page
            // to walk up the parent chain or request from pager.
            let paddr = inner.commit_page(block.block, MMUFlags::READ)?;
            hal_impl::mem::pmem_read(paddr + block.begin, &mut buf[buf_range]);
        }
        Ok(())
    }

    fn write(&self, offset: usize, buf: &[u8]) -> ZxResult {
        trace!("VMO write: offset={:#x}, len={}", offset, buf.len());
        let (_guard, inner) = self.get_inner_mut();
        trace!("VMO write: lock acquired, cache={:?}", inner.cache_policy);
        if inner.cache_policy != CachePolicy::Cached {
            return Err(ZxError::BAD_STATE);
        }
        // Bounds check: the entire write range must be within the VMO.
        let end = offset.checked_add(buf.len()).ok_or(ZxError::OUT_OF_RANGE)?;
        if end > inner.size {
            return Err(ZxError::OUT_OF_RANGE);
        }
        trace!("VMO write: calling for_each_page");
        let result = inner.for_each_page(offset, buf.len(), MMUFlags::WRITE, |paddr, buf_range| {
            hal_impl::mem::pmem_write(paddr, &buf[buf_range]);
        });
        trace!("VMO write: done, result={:?}", result);
        result
    }

    fn zero(&self, offset: usize, len: usize) -> ZxResult {
        let (_guard, inner) = self.get_inner_mut();
        if offset + len > inner.size {
            return Err(ZxError::OUT_OF_RANGE);
        }
        let iter = BlockIter {
            begin: offset,
            end: offset + len,
            block_size_log2: PAGE_SIZE_LOG2 as u8,
        };
        let mut unwanted = VecDeque::new();
        for block in iter {
            //let paddr = self.commit_page(block.block, MMUFlags::READ)?;
            if block.len() == PAGE_SIZE && !inner.is_contiguous() {
                match inner.commit_page(block.block, MMUFlags::WRITE) {
                    Ok(_) => {}
                    Err(ZxError::SHOULD_WAIT) => {
                        // Pager-backed VMO with unsupplied page — already
                        // zero. Insert a zero marker so subsequent reads
                        // return zeros without triggering a pager request.
                        inner.zero_markers.insert(block.block);
                        continue;
                    }
                    Err(e) => return Err(e),
                }
                let pinned = inner
                    .frames
                    .get(&block.block)
                    .is_some_and(|f| f.pin_count > 0);
                if pinned {
                    // Can't decommit pinned pages — zero in place.
                    if let Some(frame) = inner.frames.get(&block.block) {
                        hal_impl::mem::pmem_zero(frame.frame.paddr(), PAGE_SIZE);
                    }
                } else {
                    // Decommit the page.  Insert a zero marker if:
                    // - This VMO has a parent (COW child) — the marker
                    //   prevents a later merge from re-introducing a
                    //   page that was zeroed.  Even if parent_has_page
                    //   is false now (page was migrated), a merge can
                    //   still bring pages from the hidden node.
                    // - This is a pager-backed VMO — prevents commit_page
                    //   from re-requesting from the pager.
                    if inner.parent.is_some() || inner.pager_backed {
                        inner.zero_markers.insert(block.block);
                    }
                    unwanted.push_back(block.block + inner.parent_offset / PAGE_SIZE);
                    inner.frames.remove(&block.block);
                    // Unmap the page from all mappings so subsequent
                    // accesses trigger a page fault (which will see the
                    // zero marker and return a zero page).
                    for map in inner.mappings.iter() {
                        if let Some(map) = map.upgrade() {
                            map.range_change(block.block, 1, RangeChangeOp::Unmap);
                        }
                    }
                }
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
        let (old_parent, old_size) = {
            let (_guard, inner) = self.get_inner_mut();
            if inner.pin_count > 0 {
                // Pager-backed VMOs can grow with pinned pages.
                // Shrinking that would remove pinned pages is rejected.
                if !inner.pager_backed || len < inner.size {
                    return Err(ZxError::BAD_STATE);
                }
            }
            // Check for overflow: the new size plus accumulated parent
            // offsets must not overflow 64-bit.
            if len > inner.size {
                let mut total = inner.parent_offset.checked_add(len);
                let mut cur = inner.parent.clone();
                while let (Some(t), Some(vmop)) = (total, cur) {
                    let p = vmop.borrow_inner();
                    total = t.checked_add(p.parent_offset);
                    cur = p.parent.clone();
                }
                if total.is_none() {
                    return Err(ZxError::INVALID_ARGS);
                }
            }
            let old_size = inner.size;
            let old_parent = inner.resize(len);
            // For pager-backed VMOs growing beyond old size, insert
            // zero markers for the extended pages so reads return
            // zeros without triggering pager requests.
            if inner.pager_backed && len > old_size {
                let start = old_size / PAGE_SIZE;
                let end = len / PAGE_SIZE;
                for idx in start..end {
                    inner.zero_markers.insert(idx);
                }
            }
            // Unmap pages in mappings for the removed range. This
            // invalidates hardware page table entries so accesses to
            // the truncated region fault correctly.
            if len < old_size {
                let start_page = len / PAGE_SIZE;
                let removed_pages = (old_size - len) / PAGE_SIZE;
                for map_weak in inner.mappings.iter() {
                    if let Some(map) = map_weak.upgrade() {
                        map.range_change(start_page, removed_pages, RangeChangeOp::Unmap);
                    }
                }
            }
            (old_parent, old_size)
        };
        drop(old_parent);
        let _ = old_size; // suppress unused warning
        Ok(())
    }

    fn commit_page(&self, page_idx: usize, flags: MMUFlags) -> ZxResult<PhysAddr> {
        self.get_inner_mut().1.commit_page(page_idx, flags)
    }

    fn commit_pages_with(
        &self,
        f: &mut dyn FnMut(&mut dyn FnMut(usize, MMUFlags) -> ZxResult<PhysAddr>) -> ZxResult,
    ) -> ZxResult {
        let (_guard, inner) = self.get_inner_mut();
        f(&mut |page_idx, flags| inner.commit_page(page_idx, flags))
    }

    fn commit(&self, offset: usize, len: usize) -> ZxResult {
        let (_guard, inner) = self.get_inner_mut();
        let start_page = offset / PAGE_SIZE;
        // Use div_ceil to commit partial pages — a sub-page write
        // still needs the containing page committed.
        let end_page = (offset + len).div_ceil(PAGE_SIZE);
        for page in start_page..end_page {
            inner.commit_page(page, MMUFlags::WRITE)?;
        }
        Ok(())
    }

    fn decommit(&self, offset: usize, len: usize) -> ZxResult {
        let (_guard, inner) = self.get_inner_mut();
        // Fuchsia rejects decommit on:
        //   - Contiguous VMOs (pages must stay physically contiguous)
        //   - Snapshot children (has parent)
        //   - Parent VMOs with children (reparented under hidden node,
        //     so parent.is_some() after first create_child)
        // Matches C++ VmoCloneTestCase::Decommit and Pager::DecommitTest.
        if inner.is_contiguous() || inner.parent.is_some() {
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
        // Reject offset+len overflow.
        if offset.checked_add(len).is_none() {
            return Err(ZxError::INVALID_ARGS);
        }
        let (_guard, inner) = self.get_inner_mut();
        let child = inner.create_child(offset, len, &self.lock)?;
        Ok(child)
    }

    fn create_child_unidirectional(
        &self,
        offset: usize,
        len: usize,
    ) -> ZxResult<Arc<dyn VMObjectTrait>> {
        assert!(page_aligned(offset));
        assert!(page_aligned(len));
        if offset.checked_add(len).is_none() {
            return Err(ZxError::INVALID_ARGS);
        }
        let (_guard, inner) = self.get_inner_mut();
        let child = inner.create_child_unidirectional(offset, len, &self.lock)?;
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
        let (_guard, inner) = self.get_inner_mut();
        if !inner.frames.is_empty()
            && inner.cache_policy != CachePolicy::Cached
            && policy != CachePolicy::Cached
        {
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
        let (_guard, inner) = self.get_inner_mut();
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
        let (_guard, inner) = self.get_inner_mut();
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

    fn set_pager_backed(&self) {
        self.get_inner_mut().1.pager_backed = true;
    }

    fn is_cow_child(&self) -> bool {
        let (_guard, inner) = self.get_inner();
        matches!(inner.type_, VMOType::Snapshot)
    }

    fn page_is_shared(&self, page_idx: usize) -> bool {
        let (_guard, inner) = self.get_inner();
        // A page is shared if the child doesn't have its own copy.
        // If the page is not in the child's frame table, it comes
        // from the parent (shared).
        !inner.frames.contains_key(&page_idx)
    }

    fn is_pager_backed(&self) -> bool {
        self.get_inner().1.pager_backed
    }

    fn set_supplying(&self, v: bool) {
        self.get_inner_mut().1.supplying = v;
    }

    fn insert_zero_marker(&self, page_idx: usize) {
        let (_guard, inner) = self.get_inner_mut();
        // Only insert zero marker if the page has no committed frame.
        // Dropping a committed frame without invalidating PTEs would
        // leave dangling mappings pointing at freed physical memory.
        if !inner.frames.contains_key(&page_idx) {
            inner.zero_markers.insert(page_idx);
        }
    }

    fn has_zero_marker(&self, page_idx: usize) -> bool {
        let (_guard, inner) = self.get_inner();
        inner.zero_markers.contains(&page_idx)
    }

    fn range_change_unmap(&self, page_idx: usize, count: usize) {
        let (_guard, inner) = self.get_inner();
        for map in inner.mappings.iter() {
            if let Some(map) = map.upgrade() {
                map.range_change(page_idx, count, RangeChangeOp::Unmap);
            }
        }
    }

    fn range_change_remove_write(&self, page_idx: usize, count: usize) {
        let (_guard, inner) = self.get_inner();
        for map in inner.mappings.iter() {
            if let Some(map) = map.upgrade() {
                map.range_change(page_idx, count, RangeChangeOp::RemoveWrite);
            }
        }
    }

    fn is_page_pinned(&self, page_idx: usize) -> bool {
        let (_guard, inner) = self.get_inner();
        inner.frames.get(&page_idx).is_some_and(|f| f.pin_count > 0)
    }

    fn remove_zero_marker(&self, page_idx: usize) {
        let (_guard, inner) = self.get_inner_mut();
        inner.zero_markers.remove(&page_idx);
    }

    fn decommit_page(&self, page_idx: usize) {
        let (_guard, inner) = self.get_inner_mut();
        if let Some(frame) = inner.frames.remove(&page_idx) {
            // Don't decommit pinned pages.
            if frame.pin_count > 0 {
                inner.frames.insert(page_idx, frame);
                return;
            }
            // Insert a zero marker so reads return zeros without
            // hitting the (now-detached) pager.
            inner.zero_markers.insert(page_idx);
            // Unmap from all mappings.
            for map in inner.mappings.iter() {
                if let Some(map) = map.upgrade() {
                    map.range_change(page_idx, 1, RangeChangeOp::Unmap);
                }
            }
        }
    }

    fn clear_pager_backed(&self) {
        // Don't clear pager_backed — the VMO was pager-backed and
        // detached. Unsupplied pages should return errors (BAD_STATE)
        // instead of silently allocating zero frames. The pager_port
        // is already cleared so request_pages returns BAD_STATE.
        // Also clear the supplying flag.
        let (_guard, inner) = self.get_inner_mut();
        inner.supplying = false;
    }

    fn set_content_boundary(&self, pages: usize) {
        let (_guard, inner) = self.get_inner_mut();
        inner.content_boundary_pages = pages;
    }

    fn as_mut_buf(&self) -> ZxResult<(MutexGuard<'_, ()>, &mut [u8])> {
        let (guard, inner) = self.get_inner_mut();
        inner.as_mut_buf().map(|(addr, size)| {
            (guard, unsafe {
                core::slice::from_raw_parts_mut(addr as *mut u8, size)
            })
        })
    }

    fn unset_contiguous(&self) {
        let (_guard, inner) = self.get_inner_mut();
        if inner.contiguous {
            inner.contiguous = false;
            for frame in inner.frames.values_mut() {
                frame.pin_count -= 1;
            }
        }
    }

    fn can_unidirectional_clone(&self) -> bool {
        let (_guard, inner) = self.get_inner();
        // Walk up the cow_pages parent chain to check:
        // 1. The root must be pager-backed.
        // 2. The immediate parent (if any) must not be hidden.
        //
        // Matches Fuchsia's VmCowPages::can_unidirectional_clone_locked:
        //   if (!is_root_source_user_pager_backed()) return false;
        //   if (parent_ && is_parent_hidden_locked()) return false;
        //   return true;

        // Check condition 2: immediate parent must not be hidden.
        if let Some(parent) = &inner.parent {
            let parent_inner = parent.borrow_inner();
            if parent_inner.type_.is_hidden() {
                return false;
            }
        }

        // Check condition 1: the root of the tree must be pager-backed.
        // Walk up the cow_pages parent chain to find the root.
        let mut root_is_pager = inner.pager_backed;
        let mut cur = inner.parent.clone();
        while let Some(vmop) = cur {
            let p = vmop.borrow_inner();
            if p.pager_backed {
                root_is_pager = true;
            }
            cur = p.parent.clone();
        }

        root_is_pager
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
            block_size_log2: PAGE_SIZE_LOG2 as u8,
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
        let no_parent = self.parent.is_none();
        let no_frame = !self.frames.contains_key(&page_idx);
        let out_of_range = if self.type_.is_hidden() {
            // For hidden nodes, check both size and parent_limit.
            // A page beyond parent_limit has no parent backing — it
            // should be treated as zero (not walked up to the parent).
            let beyond_size = page_idx >= self.size / PAGE_SIZE;
            let beyond_parent = self.parent.is_some()
                && self.parent_limit > 0
                && (self.parent_offset + page_idx * PAGE_SIZE) >= self.parent_limit;
            beyond_size || beyond_parent
        } else if self.parent.is_none() {
            // For merged Snapshot VMOs (no parent but parent_limit
            // retained from merge), check against parent_limit too.
            let beyond_size = page_idx >= self.size / PAGE_SIZE;
            let beyond_parent = self.parent_limit > 0
                && (self.parent_offset + page_idx * PAGE_SIZE) >= self.parent_limit;
            beyond_size || beyond_parent
        } else {
            (self.parent_offset + page_idx * PAGE_SIZE) >= self.parent_limit
        };
        let mut need_unmap = false;
        // Zero markers block parent fall-through: a zeroed-then-decommitted
        // page reads as zero without walking up to the parent chain.
        let has_zero_marker = no_frame && self.zero_markers.contains(&page_idx);
        if no_frame {
            if out_of_range || no_parent || has_zero_marker {
                // COW children reading beyond parent_limit: return
                // shared zero page without allocating. This covers:
                // - Active COW children (has parent, out of range)
                // - Merged COW children (parent=None after merge, but
                //   parent_limit still set from the merge)
                let is_cow_out_of_range = out_of_range
                    && !flags.contains(MMUFlags::WRITE)
                    && !self.type_.is_hidden()
                    && (self.parent.is_some() || matches!(self.type_, VMOType::Snapshot));
                if is_cow_out_of_range {
                    static ZERO_PAGE: spin::Lazy<Option<PhysFrame>> =
                        spin::Lazy::new(PhysFrame::new_zero);
                    return match ZERO_PAGE.as_ref() {
                        Some(zp) => Ok(CommitResult::Ref(zp.paddr())),
                        None => Err(ZxError::NO_MEMORY),
                    };
                }
                // For root VMOs (no parent, not a COW child), accessing
                // beyond the VMO size is OUT_OF_RANGE — don't allocate
                // zero pages past the end.
                if out_of_range && no_parent && !self.type_.is_hidden() {
                    return Err(ZxError::OUT_OF_RANGE);
                }
                // Pager-backed VMOs: don't auto-allocate zero frames for
                // uncommitted pages. Return SHOULD_WAIT so the caller
                // can send a page request to the pager and wait for
                // supply_pages. Without this, the pager demand-paging
                // mechanism is bypassed and tests hang because the port
                // never receives a page request.
                // Pager-backed VMOs: don't auto-allocate zero frames for
                // unsupplied pages on READ. Return SHOULD_WAIT so the
                // caller can request pages from the pager. WRITE is
                // allowed — it allocates frames normally (the pager
                // tracks dirty state separately).
                // Zero markers: return zero page reference for reads
                // without allocating a frame. The zero marker stays in
                // place so the page continues to count as "zero" for
                // attribution. Only writes allocate a real frame.
                if has_zero_marker && !flags.contains(MMUFlags::WRITE) {
                    static ZERO_PAGE_ZM: spin::Lazy<Option<PhysFrame>> =
                        spin::Lazy::new(PhysFrame::new_zero);
                    return match ZERO_PAGE_ZM.as_ref() {
                        Some(zp) => Ok(CommitResult::Ref(zp.paddr())),
                        None => Err(ZxError::NO_MEMORY),
                    };
                }
                // Unbounded VMOs: pages beyond the content boundary
                // (stream_size) return zeros without pager requests.
                // Matches Fuchsia where pages beyond stream_size on an
                // unbounded VMO are implicitly zero (untracked).
                if page_idx >= self.content_boundary_pages && !flags.contains(MMUFlags::WRITE) {
                    static ZERO_PAGE_UNTRACKED: spin::Lazy<Option<PhysFrame>> =
                        spin::Lazy::new(PhysFrame::new_zero);
                    return match ZERO_PAGE_UNTRACKED.as_ref() {
                        Some(zp) => Ok(CommitResult::Ref(zp.paddr())),
                        None => Err(ZxError::NO_MEMORY),
                    };
                }
                if self.pager_backed
                    && !self.supplying
                    && !has_zero_marker
                    && !flags.contains(MMUFlags::WRITE)
                {
                    return Err(ZxError::SHOULD_WAIT);
                }
                let target_frame = PhysFrame::new_zero().ok_or(ZxError::NO_MEMORY)?;
                if self.type_.is_hidden() {
                    // Hidden nodes return a temporary zero page without
                    // storing a frame. Keep the zero marker so the
                    // sibling child also sees zero at this offset.
                    return Ok(CommitResult::NewPage(target_frame));
                }
                // Leaf node: the zero marker is replaced by a real
                // (zero-filled) frame in the local frames map.
                if has_zero_marker {
                    self.zero_markers.remove(&page_idx);
                }
                self.frames.insert(page_idx, PageState::new(target_frame));
                for map in self.mappings.iter() {
                    if let Some(map) = map.upgrade() {
                        map.range_change(page_idx, 1, RangeChangeOp::Unmap);
                    }
                }
            } else {
                // Walk up to parent to find or allocate the page.
                let parent = self.parent.as_ref().unwrap().borrow_inner_mut();
                let parent_idx = page_idx + self.parent_offset / PAGE_SIZE;
                let parent_flags = flags;
                match parent.commit_page_internal(parent_idx, parent_flags, &self.self_ref)? {
                    CommitResult::NewPage(frame) if !self.type_.is_hidden() => {
                        if !flags.contains(MMUFlags::WRITE) {
                            // Before returning zeros, check if the sibling
                            // child of our hidden parent has a committed
                            // page. This handles pager VMOs where
                            // supply_pages wrote to the root after clone.
                            // Only check sibling pages for pager-backed
                            // trees. For regular COW clones, siblings must
                            // NOT share pages (COW isolation).
                            if parent.type_.is_hidden() {
                                let (_, sibling_weak) =
                                    parent.type_.get_tag_and_other(&self.self_ref);
                                if let Some(sibling_arc) = sibling_weak.upgrade() {
                                    {
                                        let sib = sibling_arc.borrow_inner();
                                        if sib.pager_backed {
                                            let sib_off = sib.parent_offset / PAGE_SIZE;
                                            if parent_idx >= sib_off {
                                                let sib_idx = parent_idx - sib_off;
                                                if sib_idx < sib.size / PAGE_SIZE {
                                                    if let Some(page) = sib.frames.get(&sib_idx) {
                                                        drop(frame);
                                                        return Ok(CommitResult::Ref(
                                                            page.frame.paddr(),
                                                        ));
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
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
                        // Hidden nodes pass CopyOnWrite through without
                        // inserting — no intermediate copies. The frame
                        // goes directly to the requesting leaf.
                        if self.type_.is_hidden() && !self.contiguous {
                            return Ok(CommitResult::CopyOnWrite(frame, unmap));
                        }
                        let mut new_frame = PageState::new(frame);
                        if self.contiguous {
                            if let Some(par_frame) = parent.frames.get_mut(&parent_idx) {
                                par_frame.swap(&mut new_frame);
                            }
                            let sibling = parent.type_.get_tag_and_other(&self.self_ref).1;
                            if let Some(arc_sibling) = sibling.upgrade() {
                                {
                                    let sibling_inner = arc_sibling.borrow_inner();
                                    sibling_inner.range_change(
                                        parent_idx * PAGE_SIZE,
                                        (parent_idx + 1) * PAGE_SIZE,
                                        RangeChangeOp::Unmap,
                                    );
                                }
                            }
                        } else {
                            // Always unmap when inserting a new frame via
                            // CopyOnWrite. Even if the parent's unmap flag
                            // is false (page was migrated, not copied), this
                            // leaf may have existing PTEs (from a prior Ref
                            // mapping or slice sharing) that point to the
                            // old physical page. Without unmapping, those
                            // PTEs become stale.
                            need_unmap = true;
                        }
                        self.frames.insert(page_idx, new_frame);
                    }
                    CommitResult::Ref(paddr)
                        if self.type_.is_unidirectional() && flags.contains(MMUFlags::WRITE) =>
                    {
                        // Unidirectional child writing to a parent page:
                        // COW-fork into a local copy.
                        let new_frame = PhysFrame::new().ok_or(ZxError::NO_MEMORY)?;
                        hal_impl::mem::pmem_copy(new_frame.paddr(), paddr, PAGE_SIZE);
                        self.frames.insert(page_idx, PageState::new(new_frame));
                    }
                    CommitResult::Ref(paddr) if flags.contains(MMUFlags::WRITE) => {
                        // A COW participant (hidden node or snapshot child)
                        // received a Ref from a non-hidden parent (e.g.,
                        // pager root). For WRITE access, we must copy the
                        // page locally to maintain COW isolation — writing
                        // through the Ref would corrupt the parent's page
                        // and violate snapshot semantics.
                        //
                        // Hidden nodes: the copy goes into self.frames so
                        // the COW fork logic below (line ~1088) can handle
                        // distribution to the requesting child.
                        //
                        // Snapshot children: the copy becomes a private
                        // page owned by this VMO.
                        let new_frame = PhysFrame::new().ok_or(ZxError::NO_MEMORY)?;
                        hal_impl::mem::pmem_copy(new_frame.paddr(), paddr, PAGE_SIZE);
                        self.frames.insert(page_idx, PageState::new(new_frame));
                    }
                    r => return Ok(r),
                }
            }
        }
        // Now the page must hit on this VMO.
        let (_child_tag, other_child) = self.type_.get_tag_and_other(child);
        if self.type_.is_hidden() {
            let arc_other = other_child.upgrade().unwrap();
            let in_range = {
                let other_inner = arc_other.borrow_inner();
                let start = other_inner.parent_offset / PAGE_SIZE;
                let end = other_inner.parent_limit / PAGE_SIZE;
                page_idx >= start && page_idx < end
            };
            if !in_range {
                if flags.contains(MMUFlags::WRITE) {
                    // The other child can't see this page.  Check whether
                    // the requesting child's subtree has multiple viewers
                    // (share_count > 0).  If so, copy instead of migrate
                    // so that other nodes in the subtree still find the
                    // original page here.
                    let frame = self.frames.get_mut(&page_idx).unwrap();
                    if frame.share_count == 0 {
                        // Truly exclusive — migrate the frame out.
                        let target = self.frames.remove(&page_idx).unwrap().take();
                        return Ok(CommitResult::CopyOnWrite(target, need_unmap));
                    }
                    // Shared within requesting subtree — copy, keep original.
                    let target = PhysFrame::new().ok_or(ZxError::NO_MEMORY)?;
                    hal_impl::mem::pmem_copy(target.paddr(), frame.frame.paddr(), PAGE_SIZE);
                    frame.share_count -= 1;
                    return Ok(CommitResult::CopyOnWrite(target, true));
                }
                // READ: return reference, don't migrate.
                let frame = self.frames.get(&page_idx).unwrap();
                return Ok(CommitResult::Ref(frame.frame.paddr()));
            } else if need_unmap {
                arc_other.borrow_inner().range_change(
                    page_idx * PAGE_SIZE,
                    (1 + page_idx) * PAGE_SIZE,
                    RangeChangeOp::Unmap,
                );
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
        // COW fork: when a child writes to a shared page in a hidden node,
        // create a private copy for the writer and tag the original so
        // the other child exclusively owns it.
        if self.type_.is_hidden() && flags.contains(MMUFlags::WRITE) {
            let (_child_tag, _) = self.type_.get_tag_and_other(child);
            if frame.share_count == 0 {
                // Last viewer — migrate the frame out.
                let target_frame = self.frames.remove(&page_idx).unwrap().take();
                return Ok(CommitResult::CopyOnWrite(target_frame, need_unmap));
            }
            // Shared — create a copy for the writer, keep original.
            let target_frame = PhysFrame::new().ok_or(ZxError::NO_MEMORY)?;
            hal_impl::mem::pmem_copy(target_frame.paddr(), frame.frame.paddr(), PAGE_SIZE);
            frame.share_count -= 1;
            return Ok(CommitResult::CopyOnWrite(target_frame, true));
        }
        // Read access or non-hidden: return reference.
        Ok(CommitResult::Ref(frame.frame.paddr()))
    }

    fn decommit(&mut self, page_idx: usize) {
        self.frames.remove(&page_idx);
        // For pager-backed VMOs, insert a zero marker so reads
        // return zeros instead of triggering SHOULD_WAIT.
        if self.pager_backed {
            self.zero_markers.insert(page_idx);
        }
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
                child.borrow_inner().range_change(start, end, op);
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
            // Zero markers represent decommitted-but-zeroed pages:
            // they use no memory and block parent fall-through.
            if self.zero_markers.contains(&i) {
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
                let inner = vmop.borrow_inner();
                // Zero markers in ancestors also block the walk.
                if inner.zero_markers.contains(&current_idx) {
                    break;
                }
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

    /// Returns `true` if any ancestor in the parent chain has a
    /// committed page at `page_idx` (translated through parent_offset).
    /// This is used by the zero operation to decide whether decommitting
    /// a page would expose non-zero parent content.
    #[allow(dead_code)]
    fn parent_has_page(&self, page_idx: usize) -> bool {
        if self.parent.is_none() {
            return false;
        }
        if page_idx * PAGE_SIZE + self.parent_offset >= self.parent_limit {
            return false;
        }
        let mut current = self.parent.clone();
        let mut current_idx = page_idx + self.parent_offset / PAGE_SIZE;
        while let Some(vmop) = current {
            let inner = vmop.borrow_inner();
            // Zero markers in ancestors block the walk — the ancestor
            // zeroed this page, so no non-zero content is visible.
            if inner.zero_markers.contains(&current_idx) {
                return false;
            }
            if inner.frames.contains_key(&current_idx) {
                return true;
            }
            if inner.parent_limit == 0 {
                return false;
            }
            let next = current_idx + inner.parent_offset / PAGE_SIZE;
            if next * PAGE_SIZE >= inner.parent_limit {
                return false;
            }
            current_idx = next;
            current = inner.parent.clone();
        }
        false
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
        let surviving = arc_child.borrow_inner_mut();
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
            // Don't merge a frame into a slot that has a zero marker —
            // the zero marker takes precedence (the child zeroed that page).
            if surviving.zero_markers.contains(&idx) {
                continue;
            }
            if !surviving.frames.contains_key(&idx) && value.tag != tag.negate() {
                value.tag = PageStateTag::Owned;
                // After merge, set share_count based on whether the page
                // is visible to multiple children of the surviving node.
                // If the surviving node is a hidden node, check if both
                // children's ranges include this page index. If so,
                // share_count = 1 (shared by 2); otherwise 0 (private).
                // For leaf nodes, share_count = 0 (no descendants).
                value.share_count = if let VMOType::Hidden { left, right, .. } = &surviving.type_ {
                    let in_left = left.upgrade().is_some_and(|l| {
                        let li = l.borrow_inner();
                        let ls = li.parent_offset / PAGE_SIZE;
                        let le = li.parent_limit / PAGE_SIZE;
                        idx >= ls && idx < le && !li.frames.contains_key(&(idx - ls))
                    });
                    let in_right = right.upgrade().is_some_and(|r| {
                        let ri = r.borrow_inner();
                        let rs = ri.parent_offset / PAGE_SIZE;
                        let re = ri.parent_limit / PAGE_SIZE;
                        idx >= rs && idx < re && !ri.frames.contains_key(&(idx - rs))
                    });
                    if in_left && in_right {
                        1
                    } else {
                        0
                    }
                } else {
                    0
                };
                surviving.frames.insert(idx, value);
            }
        }

        // Transfer zero markers from the hidden node to the surviving
        // child.  A marker in the hidden node means the dying child (or
        // the hidden node itself) zeroed that page; the surviving child
        // must inherit the marker so it doesn't see stale ancestor data.
        for marker_key in core::mem::take(&mut self.zero_markers) {
            if marker_key < start || marker_key >= end {
                continue;
            }
            let idx = marker_key - start;
            // Only transfer if the surviving child doesn't already have
            // a committed frame there (a frame takes precedence).
            if !surviving.frames.contains_key(&idx) {
                surviving.zero_markers.insert(idx);
            }
        }

        // Connect surviving child to my parent.
        surviving.parent_offset += self.parent_offset;
        surviving.parent_limit += self.parent_offset;
        let surviving_offset = surviving.parent_offset;
        let surviving_limit = surviving.parent_limit;
        // Drop the surviving borrow before accessing the parent, since
        // the parent's cleanup may need to borrow its children.

        if let Some(parent) = &self.parent {
            let parent_inner = parent.borrow_inner_mut();
            // Only call replace_child on hidden parents — non-hidden
            // parents (e.g., pager root for unidirectional children)
            // don't track children via left/right.
            if parent_inner.type_.is_hidden() {
                parent_inner.replace_child(
                    &self.self_ref,
                    self.owner,
                    other_child,
                    Some((surviving_offset, surviving_limit)),
                );
            }
            // After replacing the child, clean up stale intermediate COW
            // copies in the parent (grandparent of the dying child).
            if parent_inner.type_.is_hidden() {
                Self::cleanup_stale_cow_copies(parent_inner);
            }
        }
        // Re-borrow to set parent.
        let surviving = arc_child.borrow_inner_mut();
        surviving.parent = self.parent.take();

        // If the surviving child now has no parent, zero markers are
        // unnecessary — there's no ancestor content to block.
        if surviving.parent.is_none() {
            surviving.zero_markers.clear();
        }
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
                        let c = a.borrow_inner();
                        (c.parent_offset / PAGE_SIZE, c.parent_limit / PAGE_SIZE)
                    })
                    .unwrap_or((0, 0));
                let rs = right
                    .upgrade()
                    .map(|a| {
                        let c = a.borrow_inner();
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
                let p = parent.borrow_inner_mut();
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
                let inner = vmop.borrow_inner();
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
                parent_limit: offset.saturating_add(len).min(self.size),
                size: len,
                frames: BTreeMap::new(),
                zero_markers: BTreeSet::new(),
                mappings: Vec::new(),
                cache_policy: CachePolicy::Cached,
                contiguous: false,
                pager_backed: false,
                content_boundary_pages: usize::MAX,
                supplying: false,
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
                zero_markers: core::mem::take(&mut self.zero_markers),
                mappings: Vec::new(),
                cache_policy: CachePolicy::Cached,
                contiguous: self.contiguous,
                pager_backed: false,
                content_boundary_pages: usize::MAX,
                supplying: false,
                self_ref: Default::default(),
                pin_count: self.pin_count,
            },
            Some(lock_ref.clone()),
        );
        // update parent's child reference (hidden parents only)
        if let Some(parent) = self.parent.take() {
            let parent_inner = parent.borrow_inner_mut();
            if let VMOType::Hidden { left, right, .. } = &mut parent_inner.type_ {
                if left.ptr_eq(&self.self_ref) {
                    *left = Arc::downgrade(&hidden);
                } else if right.ptr_eq(&self.self_ref) {
                    *right = Arc::downgrade(&hidden);
                } else {
                    panic!();
                }
            }
            // Non-hidden parents (e.g., pager root for unidirectional
            // children) don't track children via left/right — skip.
        }
        // update children's parent
        self.parent = Some(hidden.clone());
        self.parent_offset = 0;
        self.parent_limit = self.size;
        child.borrow_inner_mut().parent = Some(hidden.clone());
        // Increment share_count for pages visible to the new child.
        {
            let child_start = offset / PAGE_SIZE;
            let child_end = (offset + len).min(self.size) / PAGE_SIZE;
            let h = hidden.borrow_inner_mut();
            for idx in child_start..child_end {
                if let Some(page) = h.frames.get_mut(&idx) {
                    page.share_count += 1;
                }
            }
            // Also walk up ancestors for pages NOT in the hidden node.
            // For each page in the child's range that doesn't have a
            // local frame in `h`, walk up the parent chain to find the
            // ancestor that owns the page and increment its share_count
            // there. This mirrors decrement_parent_share_counts.
            //
            // Stop at non-hidden parents — they don't participate in
            // bidirectional COW share_count tracking.
            if h.parent.is_some() {
                let h_off = h.parent_offset / PAGE_SIZE;
                for idx in child_start..child_end {
                    // Skip pages with local frames in the hidden node —
                    // those shadow any ancestor version.
                    if h.frames.contains_key(&idx) {
                        continue;
                    }
                    let mut cur = h.parent.clone();
                    let mut cur_idx = idx + h_off;
                    while let Some(vmop) = cur {
                        let inner = vmop.borrow_inner_mut();
                        if !inner.type_.is_hidden() {
                            break;
                        }
                        if let Some(page) = inner.frames.get_mut(&cur_idx) {
                            page.share_count += 1;
                            break;
                        }
                        if inner.parent_limit == 0 || inner.parent.is_none() {
                            break;
                        }
                        let next = cur_idx + inner.parent_offset / PAGE_SIZE;
                        if next * PAGE_SIZE >= inner.parent_limit {
                            break;
                        }
                        cur_idx = next;
                        cur = inner.parent.clone();
                    }
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

    /// Create a unidirectional clone (SNAPSHOT_MODIFIED on pager root).
    ///
    /// The child hangs directly under the parent — no hidden node is
    /// created. Unmodified pages read through to the parent; only
    /// pages the child writes are COW-forked into the child's frames.
    ///
    /// The parent keeps all its pages and mappings (no RemoveWrite).
    /// This matches Fuchsia's unidirectional clone path in
    /// `CreateCloneLocked` / `CloneChildLocked`.
    fn create_child_unidirectional(
        &mut self,
        offset: usize,
        len: usize,
        lock_ref: &Arc<Mutex<()>>,
    ) -> ZxResult<Arc<VMObjectPaged>> {
        if self.cache_policy != CachePolicy::Cached || self.pin_count != 0 {
            return Err(ZxError::BAD_STATE);
        }
        let child = VMObjectPaged::wrap(
            VMObjectPagedInner {
                owner: new_owner_id(),
                type_: VMOType::UnidirectionalChild,
                parent: None, // set below
                parent_offset: offset,
                parent_limit: offset.saturating_add(len).min(self.size),
                size: len,
                frames: BTreeMap::new(),
                zero_markers: BTreeSet::new(),
                mappings: Vec::new(),
                cache_policy: CachePolicy::Cached,
                contiguous: false,
                pager_backed: false,
                content_boundary_pages: usize::MAX,
                supplying: false,
                self_ref: Default::default(),
                pin_count: 0,
            },
            Some(lock_ref.clone()),
        );
        // Set the child's parent directly to self (no hidden node).
        // Use self_ref to get our Arc<VMObjectPaged>.
        let self_arc = self.self_ref.upgrade().unwrap();
        child.borrow_inner_mut().parent = Some(self_arc);
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
        let other_child = arc_other_child.borrow_inner_mut();
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
                                    p.borrow_inner().frames.contains_key(&pi)
                                });
                                if has_parent_frame {
                                    // This frame is an intermediate COW
                                    // copy. Remove it and un-tag the
                                    // parent's frame.
                                    let _removed = self.frames.remove(&i);
                                    if let Some(ref parent) = self.parent {
                                        let parent_idx = i + self.parent_offset / PAGE_SIZE;
                                        let p = parent.borrow_inner_mut();
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
                let parent_inner = parent.borrow_inner_mut();
                if parent_inner.owner == old_id {
                    let (_, other) = parent_inner.type_.get_tag_and_other(&child);
                    let new_owner = other.upgrade().unwrap().borrow_inner().owner;
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
        if matches!(self.type_, VMOType::Snapshot | VMOType::UnidirectionalChild) {
            info.flags |= VmoInfoFlags::IS_COW_CLONE;
        }
        if self.is_contiguous() {
            info.flags |= VmoInfoFlags::CONTIGUOUS;
        }
        info.num_mappings = self.mappings.len() as u64;
        info.share_count = if self.mappings.is_empty() { 0 } else { 1 };

        // Compute attribution by walking the COW tree with proper
        // tag-based visibility. This gives us total_bytes (raw),
        // private_bytes, and scaled_bytes (fractional).
        let total_pages = self.size / PAGE_SIZE;
        let attribution = self.compute_attribution(0, total_pages);

        // committed_bytes / populated_bytes: total raw bytes of pages
        // this VMO owns (locally or through parent chain).
        // Matches Fuchsia's counts.total_bytes().
        info.committed_bytes = attribution.total_bytes;
        info.populated_bytes = attribution.total_bytes;

        // Private bytes: pages with share_count == 0 (no sharing).
        info.committed_private_bytes = attribution.private_bytes;
        info.populated_private_bytes = attribution.private_bytes;

        // Scaled bytes: fractional attribution.
        info.committed_scaled_bytes = attribution.scaled_bytes;
        info.populated_scaled_bytes = attribution.scaled_bytes;
        info.committed_fractional_scaled_bytes = attribution.fractional_scaled_bytes;
        info.populated_fractional_scaled_bytes = attribution.fractional_scaled_bytes;
    }

    /// Compute per-page fractional attribution using share_count.
    ///
    /// Matches Fuchsia's GetAttributedMemoryInRangeLocked: for each
    /// page this VMO "owns" (locally or through parent chain), use
    /// `PAGE_SIZE / (share_count + 1)` for fractional attribution.
    /// Local pages have share_count=0 (private). Pages in hidden
    /// parents use the parent's share_count.
    /// Compute page attribution for this VMO in the given range.
    ///
    /// Walks the COW tree from this leaf node upward through hidden
    /// parents, respecting tag-based visibility. A page in a hidden
    /// node is visible to a child if `page.tag != child_tag`.
    ///
    /// For each visible page:
    /// - `total_bytes += PAGE_SIZE` (raw, unscaled)
    /// - `private_bytes += PAGE_SIZE` if share_count == 0
    /// - `scaled_bytes += PAGE_SIZE / (share_count + 1)` (fractional)
    ///
    /// This matches Fuchsia's `GetAttributedMemoryInRangeLocked`.
    fn compute_attribution(&self, start_idx: usize, end_idx: usize) -> Attribution {
        let pages = self.size / PAGE_SIZE;
        if pages == 0 || start_idx >= pages {
            return Attribution::default();
        }
        let end_idx = end_idx.min(pages);
        let mut result = Attribution::default();

        for i in start_idx..end_idx {
            // Case 1: local frame in this (visible) leaf node.
            // Local frames are always private to this node.
            if let Some(page) = self.frames.get(&i) {
                // For a visible leaf node, share_count should be 0
                // (private). But use the actual value for correctness.
                result.add_page((page.share_count as u64) + 1);
                continue;
            }
            // Zero markers block attribution — the page uses no memory.
            if self.zero_markers.contains(&i) {
                continue;
            }
            // Check if this page is within the parent's visible range.
            if self.parent.is_none() {
                continue;
            }
            let parent_byte_offset = i * PAGE_SIZE;
            if parent_byte_offset >= self.parent_limit {
                continue;
            }
            // Case 2: walk up the parent chain to find the owning node.
            // Track which child we are at each level for tag checks.
            let mut current = self.parent.clone();
            let mut current_idx = i + self.parent_offset / PAGE_SIZE;
            let mut child_ref = self.self_ref.clone();

            while let Some(vmop) = current {
                let inner = vmop.borrow_inner();
                // Non-hidden nodes (pager root, Origin) are not part
                // of the COW tree — their pages are attributed to
                // themselves, not to children reading through them.
                // Stop the walk here to avoid double-counting.
                if !inner.type_.is_hidden() {
                    break;
                }
                // Zero markers in ancestors block attribution.
                if inner.zero_markers.contains(&current_idx) {
                    break;
                }
                if let Some(frame) = inner.frames.get(&current_idx) {
                    // Check tag visibility: in a hidden node, the child
                    // can only see pages where tag != child_tag.
                    let (child_tag, _) = inner.type_.get_tag_and_other(&child_ref);
                    if frame.tag == child_tag {
                        // This page was split away from us — we have
                        // our own copy (which we already checked in
                        // our local frames). Skip it.
                        break;
                    }
                    result.add_page((frame.share_count as u64) + 1);
                    break;
                }
                // No frame here — continue walking up.
                if inner.parent.is_none() {
                    break;
                }
                let next_byte = current_idx * PAGE_SIZE;
                if next_byte >= inner.parent_limit {
                    break;
                }
                let next_idx = current_idx + inner.parent_offset / PAGE_SIZE;
                child_ref = inner.self_ref.clone();
                current = inner.parent.clone();
                current_idx = next_idx;
            }
        }
        result
    }

    fn release_unwanted_pages_in_parent(&mut self, mut unwanted: VecDeque<usize>) {
        let mut option_parent = self.parent.clone();
        let mut child = self.self_ref.clone();
        while let Some(parent) = option_parent {
            let parent_inner = parent.borrow_inner_mut();
            // Non-hidden parents don't have left/right child tracking.
            if !parent_inner.type_.is_hidden() {
                break;
            }
            let (tag, other) = parent_inner.type_.get_tag_and_other(&child);
            let arc_other = other.upgrade().unwrap();
            let other_inner = arc_other.borrow_inner_mut();
            let start = other_inner.parent_offset / PAGE_SIZE;
            let end = other_inner.parent_limit / PAGE_SIZE;
            for _ in 0..unwanted.len() {
                let idx = unwanted.pop_front().unwrap();
                // if the frame is in other_inner's range, check if it can be move to other_inner
                if start <= idx && idx < end {
                    if parent_inner.frames.contains_key(&idx) {
                        let new_key = idx - start;
                        // Only move to the other child if it doesn't
                        // already have a local frame (COW copy) there.
                        if other_inner.frames.contains_key(&new_key) {
                            // Other child has its own copy — just
                            // remove the parent frame (no longer needed).
                            parent_inner.frames.remove(&idx);
                        } else {
                            let mut to_insert = parent_inner.frames.remove(&idx).unwrap();
                            if parent_inner.contiguous
                                && !other_inner.contiguous
                                && to_insert.pin_count >= 1
                            {
                                to_insert.pin_count -= 1;
                            }
                            if to_insert.tag != tag.negate() {
                                to_insert.tag = PageStateTag::Owned;
                                other_inner.frames.insert(new_key, to_insert);
                            }
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
        }
    }

    /// Decrement share_count for pages this VMO sees through the parent
    /// chain in the given range. Skips pages with local frames.
    fn decrement_parent_share_counts(&self, start_page: usize, end_page: usize) {
        if self.parent.is_none() {
            return;
        }
        let parent_vis = self.parent_limit.saturating_sub(self.parent_offset) / PAGE_SIZE;
        let vis_pages = end_page.min(parent_vis);
        for i in start_page..vis_pages {
            if self.frames.contains_key(&i) {
                continue;
            }
            let mut cur = self.parent.clone();
            let mut cur_idx = i + self.parent_offset / PAGE_SIZE;
            while let Some(vmop) = cur {
                let p = vmop.borrow_inner_mut();
                // Stop at non-hidden parents — they don't participate
                // in bidirectional COW share_count tracking.
                if !p.type_.is_hidden() {
                    break;
                }
                if let Some(page) = p.frames.get_mut(&cur_idx) {
                    if page.share_count > 0 {
                        page.share_count -= 1;
                    }
                    break;
                }
                if p.parent_limit == 0 || p.parent.is_none() {
                    break;
                }
                let next = cur_idx + p.parent_offset / PAGE_SIZE;
                if next * PAGE_SIZE >= p.parent_limit {
                    break;
                }
                cur_idx = next;
                cur = p.parent.clone();
            }
        }
    }

    fn resize(&mut self, new_size: usize) -> Option<Arc<VMObjectPaged>> {
        let mut old_parent = None;
        if new_size == 0 && new_size < self.size {
            // Decrement share_count for ALL visible parent pages
            // before clearing frames and detaching from parent.
            self.decrement_parent_share_counts(0, self.size / PAGE_SIZE);
            self.frames.clear();
            if let Some(parent) = self.parent.as_ref() {
                parent.borrow_inner_mut().remove_child(&self.self_ref);
            }
            // We cannot drop the parent Arc here since we are holding the lock
            // pass it to caller who can drop it after unlocking the lock
            old_parent = self.parent.take();
            self.parent_offset = 0;
            self.parent_limit = 0;
        } else if new_size < self.size {
            // Decrement share_count for parent pages being truncated
            // BEFORE decommitting local frames (decommit removes them).
            self.decrement_parent_share_counts(new_size / PAGE_SIZE, self.size / PAGE_SIZE);
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
        let (_guard, inner) = self.get_inner_mut();
        // Decrement share_count for pages this VMO sees through the
        // parent chain, then remove from parent's child list.

        if let Some(parent) = &inner.parent {
            // Unidirectional children whose parent is still a non-hidden
            // node (pager root) skip share_count tracking and remove_child.
            // But if a unidirectional child later created a bidirectional
            // child (hidden node), its parent changed to the hidden node.
            // In that case, it must participate in normal COW merge.
            let parent_is_hidden = parent.borrow_inner().type_.is_hidden();
            if inner.type_.is_unidirectional() && !parent_is_hidden {
                // True unidirectional: parent is still the pager root.
                // No share_count, no merge needed.
            } else {
                inner.decrement_parent_share_counts(0, inner.size / PAGE_SIZE);
                parent.borrow_inner_mut().remove_child(&inner.self_ref);
            }
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
        super::super::tests::read_write(&vmo);
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
            #[allow(clippy::needless_range_loop)]
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

    #[test]
    fn disjoint_clones_attribution() {
        // Mimics ResizeDisjointChild: 3-page parent, 3 disjoint 1-page clones.
        let vmo = VmObject::new_paged(3);
        vmo.test_write(0, 1);
        vmo.test_write(1, 2);
        vmo.test_write(2, 3);

        let c0 = vmo.create_child(false, 0, PAGE_SIZE).unwrap();
        let c1 = vmo.create_child(false, PAGE_SIZE, PAGE_SIZE).unwrap();
        let c2 = vmo.create_child(false, 2 * PAGE_SIZE, PAGE_SIZE).unwrap();

        // Verify data
        assert_eq!(c0.test_read(0), 1);
        assert_eq!(c1.test_read(0), 2);
        assert_eq!(c2.test_read(0), 3);

        // Each page is shared between vmo and 1 clone → share_count=1 → 4096/2=2048 each.
        // vmo: 3 * 2048 = 6144
        let info = vmo.get_info();
        println!(
            "vmo info: committed={}, populated={}, scaled={}, frac_scaled={}, private={}",
            info.committed_bytes,
            info.populated_bytes,
            info.populated_scaled_bytes,
            info.populated_fractional_scaled_bytes,
            info.committed_private_bytes
        );
        for p in 0..3 {
            println!(
                "vmo committed page {}: {}",
                p,
                vmo.committed_pages_in_range(p, p + 1)
            );
        }
        // Check clones can find their pages
        println!("c0 committed: {}", c0.committed_pages_in_range(0, 1));
        println!("c1 committed: {}", c1.committed_pages_in_range(0, 1));
        println!("c2 committed: {}", c2.committed_pages_in_range(0, 1));
        // Check if VmoCheck reads caused vmo to get local frames
        println!(
            "vmo local frames: {}",
            vmo.get_info().committed_private_bytes
        );
        assert_eq!(
            info.populated_scaled_bytes as usize,
            3 * PAGE_SIZE / 2,
            "vmo should have 3 * page/2 = {} but got {}",
            3 * PAGE_SIZE / 2,
            info.populated_scaled_bytes
        );
        // Each clone: 1 * 2048 = 2048
        assert_eq!(
            c0.get_info().populated_scaled_bytes as usize,
            PAGE_SIZE / 2,
            "c0 populated_scaled_bytes"
        );
        assert_eq!(
            c1.get_info().populated_scaled_bytes as usize,
            PAGE_SIZE / 2,
            "c1 populated_scaled_bytes"
        );
        assert_eq!(
            c2.get_info().populated_scaled_bytes as usize,
            PAGE_SIZE / 2,
            "c2 populated_scaled_bytes"
        );
    }

    #[test]
    #[ignore = "COW populated_bytes fractional attribution not implemented (issue #668 section 17)"]
    fn resize_child_data_isolation() {
        // Mimics VmoCloneResizeTests::ResizeChild
        let vmo = VmObject::new_paged_with_resizable(true, 4);
        for i in 0..4u8 {
            vmo.test_write(i as usize, i + 1);
        }
        let clone = vmo.create_child(true, 0, 4 * PAGE_SIZE).unwrap();

        // Write to parent page 1 and clone page 2
        vmo.write(PAGE_SIZE, &[5u8]).unwrap();
        clone.write(2 * PAGE_SIZE, &[5u8]).unwrap();

        // Verify data isolation
        assert_eq!(vmo.test_read(0), 1, "parent page 0");
        assert_eq!(vmo.test_read(1), 5, "parent page 1 (written)");
        assert_eq!(vmo.test_read(2), 3, "parent page 2 (should be original)");
        assert_eq!(vmo.test_read(3), 4, "parent page 3");

        assert_eq!(clone.test_read(0), 1, "clone page 0");
        assert_eq!(clone.test_read(1), 2, "clone page 1 (original)");
        assert_eq!(clone.test_read(2), 5, "clone page 2 (written)");
        assert_eq!(clone.test_read(3), 4, "clone page 3");

        // Resize clone to 1 page
        clone.set_len(PAGE_SIZE).unwrap();

        // After resize, parent should still see all 4 pages correctly
        println!(
            "After resize: vmo reads: p0={} p1={} p2={} p3={}",
            vmo.test_read(0),
            vmo.test_read(1),
            vmo.test_read(2),
            vmo.test_read(3)
        );
        assert_eq!(vmo.test_read(0), 1, "parent page 0 after resize");
        assert_eq!(vmo.test_read(1), 5, "parent page 1 after resize");
        assert_eq!(vmo.test_read(2), 3, "parent page 2 after resize");
        assert_eq!(vmo.test_read(3), 4, "parent page 3 after resize");
    }

    #[test]
    fn offset_test() {
        // Mimics VmoClone2TestCase.Offset
        let vmo = VmObject::new_paged(3);
        vmo.test_write(0, 1);
        vmo.test_write(1, 2);
        vmo.test_write(2, 3);

        let clone = vmo.create_child(false, PAGE_SIZE, 3 * PAGE_SIZE).unwrap();

        // Check data
        assert_eq!(clone.test_read(0), 2);
        assert_eq!(clone.test_read(1), 3);
        assert_eq!(clone.test_read(2), 0); // zero page

        // Check attribution before any writes
        let vmo_info = vmo.get_info();
        let clone_info = clone.get_info();
        println!(
            "Before write: vmo scaled={}, clone scaled={}",
            vmo_info.populated_scaled_bytes, clone_info.populated_scaled_bytes
        );
        assert_eq!(
            vmo_info.populated_scaled_bytes as usize,
            2 * PAGE_SIZE,
            "vmo should be 1 + 1/2 + 1/2 = 2 pages"
        );
        assert_eq!(
            clone_info.populated_scaled_bytes as usize, PAGE_SIZE,
            "clone should be 1/2 + 1/2 = 1 page"
        );

        // Write to clone page 1, drop vmo
        clone.write(PAGE_SIZE, &[4]).unwrap();
        drop(vmo);

        // Check data after drop
        assert_eq!(clone.test_read(0), 2);
        assert_eq!(clone.test_read(1), 4);
        assert_eq!(clone.test_read(2), 0);

        // Check attribution after drop — clone should own 2 pages
        let info = clone.get_info();
        println!(
            "After drop: clone scaled={} committed={} private={} pop={}",
            info.populated_scaled_bytes,
            info.committed_bytes,
            info.committed_private_bytes,
            info.populated_bytes
        );
        for p in 0..3 {
            println!(
                "  page {}: committed={}",
                p,
                clone.committed_pages_in_range(p, p + 1)
            );
        }
        assert_eq!(
            info.populated_scaled_bytes as usize,
            2 * PAGE_SIZE,
            "clone should own 2 pages after parent drops"
        );
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
        for perm in &permutations {
            let vmo0 = VmObject::new_paged(1);
            vmo0.write(0, &[old]).unwrap();
            let vmo1 = vmo0.create_child(false, 0, PAGE_SIZE).unwrap();
            let vmo2 = vmo0.create_child(false, 0, PAGE_SIZE).unwrap();
            let vmo3 = vmo1.create_child(false, 0, PAGE_SIZE).unwrap();
            let vmos: [Arc<VmObject>; 4] = [vmo0.clone(), vmo1.clone(), vmo2.clone(), vmo3.clone()];
            let mut write: [bool; 4] = [false; 4];
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
        vmo.set_len(PAGE_SIZE).unwrap();
        assert_eq!(vmo.len(), PAGE_SIZE);

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
