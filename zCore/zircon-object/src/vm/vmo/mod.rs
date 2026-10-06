use {
    self::{paged::*, physical::*, slice::*},
    super::*,
    crate::object::*,
    crate::signal::{
        PacketPageRequest, PayloadRepr, Port, PortPacketRepr, ZX_PAGER_VMO_DIRTY, ZX_PAGER_VMO_READ,
    },
    alloc::{
        sync::{Arc, Weak},
        vec::Vec,
    },
    bitflags::bitflags,
    core::ops::Deref,
    hal_impl::CachePolicy,
    lock::{Mutex, MutexGuard},
};

mod paged;
mod physical;
mod slice;

kcounter!(VMO_PAGE_ALLOC, "vmo.page_alloc");
kcounter!(VMO_PAGE_DEALLOC, "vmo.page_dealloc");

/// The amount of memory committed to VMOs.
pub fn vmo_page_bytes() -> usize {
    (VMO_PAGE_ALLOC.get() - VMO_PAGE_DEALLOC.get()) * PAGE_SIZE
}

/// Virtual Memory Object Trait
#[allow(clippy::len_without_is_empty)]
pub trait VMObjectTrait: Sync + Send {
    /// Read memory to `buf` from VMO at `offset`.
    fn read(&self, offset: usize, buf: &mut [u8]) -> ZxResult;

    /// Write memory from `buf` to VMO at `offset`.
    fn write(&self, offset: usize, buf: &[u8]) -> ZxResult;

    /// Resets the range of bytes in the VMO from `offset` to `offset+len` to 0.
    fn zero(&self, offset: usize, len: usize) -> ZxResult;

    /// Get the length of VMO.
    fn len(&self) -> usize;

    /// Set the length of VMO.
    fn set_len(&self, len: usize) -> ZxResult;

    /// Commit a page.
    fn commit_page(&self, page_idx: usize, flags: MMUFlags) -> ZxResult<PhysAddr>;

    /// Commit pages with an external function f.
    /// the vmo is internally locked before it calls f,
    /// allowing `VmMapping` to avoid deadlock
    #[allow(clippy::type_complexity)]
    fn commit_pages_with(
        &self,
        f: &mut dyn FnMut(&mut dyn FnMut(usize, MMUFlags) -> ZxResult<PhysAddr>) -> ZxResult,
    ) -> ZxResult;

    /// Commit allocating physical memory.
    fn commit(&self, offset: usize, len: usize) -> ZxResult;

    /// Decommit allocated physical memory.
    fn decommit(&self, offset: usize, len: usize) -> ZxResult;

    /// Create a child VMO.
    fn create_child(&self, offset: usize, len: usize) -> ZxResult<Arc<dyn VMObjectTrait>>;

    /// Append a mapping to the VMO's mapping list.
    fn append_mapping(&self, _mapping: Weak<VmMapping>) {}

    /// Remove a mapping from the VMO's mapping list.
    fn remove_mapping(&self, _mapping: Weak<VmMapping>) {}

    /// Complete the VmoInfo.
    fn complete_info(&self, info: &mut VmoInfo);

    /// Get the cache policy.
    fn cache_policy(&self) -> CachePolicy;

    /// Set the cache policy.
    fn set_cache_policy(&self, policy: CachePolicy) -> ZxResult;

    /// Count committed pages of the VMO.
    fn committed_pages_in_range(&self, start_idx: usize, end_idx: usize) -> usize;

    /// Pin the given range of the VMO.
    fn pin(&self, _offset: usize, _len: usize) -> ZxResult {
        Err(ZxError::NOT_SUPPORTED)
    }

    /// Unpin the given range of the VMO.
    fn unpin(&self, _offset: usize, _len: usize) -> ZxResult {
        Err(ZxError::NOT_SUPPORTED)
    }

    /// Check whether any page in [offset, offset+len) is pinned.
    fn has_pinned_pages(&self, _offset: usize, _len: usize) -> bool {
        false
    }

    /// Returns true if the object is backed by a contiguous range of physical memory.
    fn is_contiguous(&self) -> bool {
        false
    }

    /// Returns true if the object is backed by RAM.
    fn is_paged(&self) -> bool {
        false
    }

    /// If contiguous, transmute vmo to a mutable buffer
    fn as_mut_buf(&self) -> ZxResult<(MutexGuard<'_, ()>, &mut [u8])> {
        Err(ZxError::NOT_SUPPORTED)
    }

    /// Mark as not contiguous
    fn unset_contiguous(&self) {}
}

/// Virtual memory containers
///
/// ## SYNOPSIS
///
/// A Virtual Memory Object (VMO) represents a contiguous region of virtual memory
/// that may be mapped into multiple address spaces.
pub struct VmObject {
    base: KObjectBase,
    _counter: CountHelper,
    resizable: bool,
    /// True if this VMO is a slice or reference child.
    is_slice: bool,
    /// True if the VMO was created with SNAPSHOT + NO_WRITE.
    immutable: core::sync::atomic::AtomicBool,
    /// True if all handles are closed (for slice VMOs, commit_page
    /// returns NOT_FOUND to prevent re-mapping parent data).
    dead: core::sync::atomic::AtomicBool,
    trait_: Arc<dyn VMObjectTrait>,
    inner: Mutex<VmObjectInner>,
}

impl_kobject!(VmObject
    fn on_zero_handles(&self) {
        // When all handles to a slice/reference VMO are closed,
        // invalidate all page table entries and mark the VMO as
        // dead. This matches Fuchsia's OnZeroHandles behavior —
        // mapped pages become inaccessible (re-faults fail).
        if self.is_slice {
            self.dead
                .store(true, core::sync::atomic::Ordering::Release);
            let inner = self.inner.lock();
            for mapping_weak in &inner.mappings {
                if let Some(mapping) = mapping_weak.upgrade() {
                    mapping.unmap_all_pages();
                }
            }
        }
    }
);
define_count_helper!(VmObject);

#[derive(Default)]
struct VmObjectInner {
    parent: Weak<VmObject>,
    children: Vec<Weak<VmObject>>,
    mapping_count: usize,
    /// Weak references to mappings (for slice VMOs that need to
    /// invalidate page table entries on drop).
    mappings: Vec<Weak<VmMapping>>,
    content_size: usize,
    /// Pager association: port to notify on page fault, and key.
    pager_port: Option<Arc<Port>>,
    pager_key: u64,
    /// Whether this VMO traps writes for dirty-page notifications.
    trap_dirty: bool,
    /// Per-page dirty state. `true` = dirty (write allowed),
    /// `false` = clean (write traps if `trap_dirty` is set).
    /// Only used when `trap_dirty` is true.
    dirty_pages: Vec<bool>,
    /// Threads waiting for pager to supply/dirty pages. Each entry is a
    /// (page_index, sender) pair. Waiters for both READ and DIRTY
    /// requests share this list — they're woken by supply_pages or
    /// op_range(DIRTY) respectively.
    pager_waiters: Vec<(usize, futures::channel::oneshot::Sender<ZxResult>)>,
}

impl VmObject {
    /// Create a new VMO backing on physical memory allocated in pages.
    pub fn new_paged(pages: usize) -> Arc<Self> {
        Self::new_paged_with_resizable(false, pages)
    }

    /// Create a new VMO, which can be resizable, backing on physical memory allocated in pages.
    pub fn new_paged_with_resizable(resizable: bool, pages: usize) -> Arc<Self> {
        let base = KObjectBase::with_signal(Signal::VMO_ZERO_CHILDREN);
        Arc::new(VmObject {
            resizable,
            is_slice: false,
            immutable: core::sync::atomic::AtomicBool::new(false),
            dead: core::sync::atomic::AtomicBool::new(false),
            _counter: CountHelper::new(),
            trait_: VMObjectPaged::new(pages),
            inner: Mutex::new(VmObjectInner::default()),
            base,
        })
    }

    /// Create a new VMO representing a piece of contiguous physical memory.
    pub fn new_physical(paddr: PhysAddr, pages: usize) -> Arc<Self> {
        Arc::new(VmObject {
            base: KObjectBase::with_signal(Signal::VMO_ZERO_CHILDREN),
            resizable: false,
            is_slice: false,
            immutable: core::sync::atomic::AtomicBool::new(false),
            dead: core::sync::atomic::AtomicBool::new(false),
            _counter: CountHelper::new(),
            trait_: VMObjectPhysical::new(paddr, pages),
            inner: Mutex::new(VmObjectInner::default()),
        })
    }

    /// Create a VM object referring to a specific contiguous range of physical frame.
    pub fn new_contiguous(pages: usize, align_log2: usize) -> ZxResult<Arc<Self>> {
        let vmo = Arc::new(VmObject {
            base: KObjectBase::with_signal(Signal::VMO_ZERO_CHILDREN),
            resizable: false,
            is_slice: false,
            immutable: core::sync::atomic::AtomicBool::new(false),
            dead: core::sync::atomic::AtomicBool::new(false),
            _counter: CountHelper::new(),
            trait_: VMObjectPaged::new_contiguous(pages, align_log2)?,
            inner: Mutex::new(VmObjectInner::default()),
        });
        Ok(vmo)
    }

    /// Set the pager association for this VMO.
    ///
    /// When a page fault occurs on an uncommitted page, a
    /// `ZX_PAGER_VMO_READ` packet will be sent to the port.
    pub fn set_pager(&self, port: Arc<Port>, key: u64) {
        let mut inner = self.inner.lock();
        inner.pager_port = Some(port);
        inner.pager_key = key;
    }

    /// Enable dirty-page trapping for this pager-backed VMO.
    /// Writes to clean pages will send `ZX_PAGER_VMO_DIRTY` to the
    /// pager port and block until the pager responds.
    pub fn set_trap_dirty(&self, pages: usize) {
        let mut inner = self.inner.lock();
        inner.trap_dirty = true;
        inner.dirty_pages = alloc::vec![false; pages];
    }

    /// Check if this VMO has dirty-page trapping enabled.
    pub fn is_trap_dirty(&self) -> bool {
        self.inner.lock().trap_dirty
    }

    /// Send a dirty-page request to the pager for the given range.
    /// Returns `Err(SHOULD_WAIT)` — caller should await `wait_for_page`.
    pub fn request_dirty(&self, offset: usize, length: usize) -> ZxResult {
        let inner = self.inner.lock();
        if let Some(port) = &inner.pager_port {
            let page_idx = offset / PAGE_SIZE;
            // Don't send duplicate requests.
            if !inner.pager_waiters.iter().any(|(idx, _)| *idx == page_idx) {
                port.push(PortPacketRepr {
                    key: inner.pager_key,
                    status: ZxError::OK,
                    data: PayloadRepr::PageRequest(PacketPageRequest {
                        command: ZX_PAGER_VMO_DIRTY,
                        flags: 0,
                        _reserved0: 0,
                        offset: offset as u64,
                        length: length as u64,
                        _reserved1: 0,
                    }),
                });
            }
            Err(ZxError::SHOULD_WAIT)
        } else {
            Err(ZxError::NOT_FOUND)
        }
    }

    /// Mark pages as dirty (called by pager_op_range(DIRTY)).
    /// Wakes any threads waiting for dirty permission on these pages.
    pub fn mark_pages_dirty(&self, offset: usize, length: usize) {
        let mut inner = self.inner.lock();
        let start_page = offset / PAGE_SIZE;
        let end_page = (offset + length).div_ceil(PAGE_SIZE);
        for page_idx in start_page..end_page.min(inner.dirty_pages.len()) {
            inner.dirty_pages[page_idx] = true;
        }
        // Wake waiters for these pages (shared waiter list with read waiters).
        let mut i = 0;
        while i < inner.pager_waiters.len() {
            if inner.pager_waiters[i].0 >= start_page && inner.pager_waiters[i].0 < end_page {
                let (_, tx) = inner.pager_waiters.swap_remove(i);
                let _ = tx.send(Ok(()));
            } else {
                i += 1;
            }
        }
    }

    /// Check if a page is dirty (i.e., write-allowed for TRAP_DIRTY VMOs).
    pub fn is_page_dirty(&self, page_idx: usize) -> bool {
        let inner = self.inner.lock();
        if !inner.trap_dirty {
            return true; // Non-TRAP_DIRTY VMOs treat all pages as writable
        }
        inner.dirty_pages.get(page_idx).copied().unwrap_or(false)
    }

    /// For TRAP_DIRTY VMOs, find the first clean page in the range
    /// [offset, offset+length). Returns None if all pages are dirty
    /// (or if TRAP_DIRTY is not set). Returns Some(clean_offset) with
    /// the byte offset of the first clean page.
    pub fn first_clean_page_in_range(&self, offset: usize, length: usize) -> Option<usize> {
        let inner = self.inner.lock();
        if !inner.trap_dirty {
            return None;
        }
        let start_page = offset / PAGE_SIZE;
        let end_page = (offset + length).div_ceil(PAGE_SIZE);
        let max_page = end_page.min(inner.dirty_pages.len());
        if start_page >= max_page {
            // All pages in range are beyond the bitmap — they're clean.
            return if start_page < end_page {
                Some(start_page * PAGE_SIZE)
            } else {
                None
            };
        }
        for (i, dirty) in inner.dirty_pages[start_page..max_page].iter().enumerate() {
            if !dirty {
                return Some((start_page + i) * PAGE_SIZE);
            }
        }
        // Pages beyond dirty_pages vec are clean.
        if end_page > inner.dirty_pages.len() && start_page < end_page {
            let first_beyond = inner.dirty_pages.len().max(start_page);
            return Some(first_beyond * PAGE_SIZE);
        }
        None
    }

    /// Check if this VMO is pager-backed.
    pub fn is_pager_backed(&self) -> bool {
        self.inner.lock().pager_port.is_some()
    }

    /// Query dirty page ranges within [offset, offset+length).
    /// Returns a list of (offset, length, options) tuples.
    /// Only returns data for TRAP_DIRTY VMOs; others return empty.
    pub fn query_dirty_ranges(&self, offset: usize, length: usize) -> Vec<(u64, u64, u64)> {
        let inner = self.inner.lock();
        if !inner.trap_dirty {
            return Vec::new();
        }
        let start_page = offset / PAGE_SIZE;
        let end_page = (offset + length).div_ceil(PAGE_SIZE);
        let max_page = end_page.min(inner.dirty_pages.len());
        if start_page >= max_page {
            return Vec::new();
        }
        let mut ranges = Vec::new();
        let mut run_start: Option<usize> = None;
        for (i, &dirty) in inner.dirty_pages[start_page..max_page].iter().enumerate() {
            let page_idx = start_page + i;
            if dirty {
                if run_start.is_none() {
                    run_start = Some(page_idx);
                }
            } else if let Some(start) = run_start {
                ranges.push((
                    (start * PAGE_SIZE) as u64,
                    ((page_idx - start) * PAGE_SIZE) as u64,
                    0u64,
                ));
                run_start = None;
            }
        }
        if let Some(start) = run_start {
            ranges.push((
                (start * PAGE_SIZE) as u64,
                ((max_page - start) * PAGE_SIZE) as u64,
                0u64,
            ));
        }
        ranges
    }

    /// Clear the pager association (called on detach).
    pub fn clear_pager(&self) {
        let mut inner = self.inner.lock();
        inner.pager_port = None;
        inner.pager_key = 0;
    }

    /// Send a page request to the pager for the given offset/length.
    /// Returns `Err(SHOULD_WAIT)` — the caller should then call
    /// `wait_for_page` to block until the page is supplied.
    pub fn request_pages(&self, offset: usize, length: usize) -> ZxResult {
        let inner = self.inner.lock();
        if let Some(port) = &inner.pager_port {
            let page_idx = offset / PAGE_SIZE;
            // Don't send duplicate port requests for the same page.
            if !inner.pager_waiters.iter().any(|(idx, _)| *idx == page_idx) {
                port.push(PortPacketRepr {
                    key: inner.pager_key,
                    status: ZxError::OK,
                    data: PayloadRepr::PageRequest(PacketPageRequest {
                        command: ZX_PAGER_VMO_READ,
                        flags: 0,
                        _reserved0: 0,
                        offset: offset as u64,
                        length: length as u64,
                        _reserved1: 0,
                    }),
                });
                info!(
                    "pager: requested page at offset={:#x} len={:#x}",
                    offset, length
                );
            }
            Err(ZxError::SHOULD_WAIT)
        } else {
            Err(ZxError::NOT_FOUND)
        }
    }

    /// Block until the page at `offset` is supplied by the pager.
    /// Returns `Ok(())` when the page is ready, or `Err` if the
    /// pager is detached.
    pub async fn wait_for_page(&self, offset: usize) -> ZxResult {
        let rx = {
            let mut inner = self.inner.lock();
            let page_idx = offset / PAGE_SIZE;
            // If the page was already supplied between request_pages()
            // and now (race window), return immediately.
            if self.committed_pages_in_range(page_idx, page_idx + 1) > 0 {
                return Ok(());
            }
            let (tx, rx) = futures::channel::oneshot::channel();
            inner.pager_waiters.push((page_idx, tx));
            rx
        };
        match rx.await {
            Ok(result) => result,
            Err(_) => Err(ZxError::CANCELED), // sender dropped
        }
    }

    /// Wake threads waiting for pages in the given range.
    /// Called by `Pager::supply_pages` after writing page data.
    pub fn complete_pager_requests(&self, offset: usize, length: usize) {
        let mut inner = self.inner.lock();
        let start_page = offset / PAGE_SIZE;
        let end_page = (offset + length).div_ceil(PAGE_SIZE);
        // Drain waiters whose page index falls in [start_page, end_page).
        let mut i = 0;
        while i < inner.pager_waiters.len() {
            if inner.pager_waiters[i].0 >= start_page && inner.pager_waiters[i].0 < end_page {
                let (_, tx) = inner.pager_waiters.swap_remove(i);
                let _ = tx.send(Ok(()));
            } else {
                i += 1;
            }
        }
    }

    /// Wake all pager waiters with an error (called on detach/destroy).
    pub fn fail_pager_requests(&self, err: ZxError) {
        let mut inner = self.inner.lock();
        for (_, tx) in inner.pager_waiters.drain(..) {
            let _ = tx.send(Err(err));
        }
    }

    /// Wake pager waiters in a specific range with an error
    /// (called by pager_op_range(FAIL)).
    pub fn fail_pager_requests_range(&self, offset: usize, length: usize, err: ZxError) {
        let mut inner = self.inner.lock();
        let start_page = offset / PAGE_SIZE;
        let end_page = (offset + length).div_ceil(PAGE_SIZE);
        let mut i = 0;
        while i < inner.pager_waiters.len() {
            if inner.pager_waiters[i].0 >= start_page && inner.pager_waiters[i].0 < end_page {
                let (_, tx) = inner.pager_waiters.swap_remove(i);
                let _ = tx.send(Err(err));
            } else {
                i += 1;
            }
        }
    }

    /// Create a child VMO.
    pub fn create_child(
        self: &Arc<Self>,
        resizable: bool,
        offset: usize,
        len: usize,
    ) -> ZxResult<Arc<Self>> {
        let base = KObjectBase::with_signal(Signal::VMO_ZERO_CHILDREN);
        base.set_name(&self.base.name());
        let trait_ = self.trait_.create_child(offset, len)?;
        let child = Arc::new(VmObject {
            base,
            resizable,
            is_slice: false,
            immutable: core::sync::atomic::AtomicBool::new(false), // Caller sets this after creation if needed
            dead: core::sync::atomic::AtomicBool::new(false),
            _counter: CountHelper::new(),
            trait_,
            inner: Mutex::new(VmObjectInner {
                parent: Arc::downgrade(self),
                ..VmObjectInner::default()
            }),
        });
        self.add_child(&child);
        Ok(child)
    }

    /// Create a child slice as an VMO
    /// Create a slice (sub-VMO) of this VMO.
    ///
    /// If `allow_resizable_parent` is true, the resizable-parent check
    /// is skipped. This is used for ZX_VMO_CHILD_REFERENCE which creates
    /// a full-VMO alias that is allowed on resizable VMOs.
    pub fn create_slice(self: &Arc<Self>, offset: usize, p_size: usize) -> ZxResult<Arc<Self>> {
        self.create_slice_inner(offset, p_size, false)
    }

    /// Create a slice that is allowed on resizable parents (REFERENCE child).
    pub fn create_reference_slice(
        self: &Arc<Self>,
        offset: usize,
        p_size: usize,
    ) -> ZxResult<Arc<Self>> {
        self.create_slice_inner(offset, p_size, true)
    }

    fn create_slice_inner(
        self: &Arc<Self>,
        offset: usize,
        p_size: usize,
        allow_resizable_parent: bool,
    ) -> ZxResult<Arc<Self>> {
        let size = roundup_pages(p_size);
        // why 32 * PAGE_SIZE? Refered to zircon source codes
        if size < p_size || size > usize::MAX & !(32 * PAGE_SIZE) {
            return Err(ZxError::OUT_OF_RANGE);
        }
        // child slice must be wholly contained
        let parent_size = self.trait_.len();
        if !page_aligned(offset) {
            return Err(ZxError::INVALID_ARGS);
        }
        if offset > parent_size || size > parent_size - offset {
            return Err(ZxError::INVALID_ARGS);
        }
        if self.resizable && !allow_resizable_parent {
            return Err(ZxError::NOT_SUPPORTED);
        }
        if self.trait_.cache_policy() != CachePolicy::Cached && !self.trait_.is_contiguous() {
            return Err(ZxError::BAD_STATE);
        }
        // Copy content_size from parent (clamped to slice range) so
        // streams on REFERENCE children see the correct size.
        let parent_content_size = self.content_size();
        let child_content_size = parent_content_size.saturating_sub(offset).min(size);
        let child = Arc::new(VmObject {
            base: KObjectBase::with(&self.base.name(), Signal::VMO_ZERO_CHILDREN),
            resizable: false,
            is_slice: true,
            immutable: core::sync::atomic::AtomicBool::new(false),
            dead: core::sync::atomic::AtomicBool::new(false),
            _counter: CountHelper::new(),
            trait_: VMObjectSlice::new(self.trait_.clone(), offset, size),
            inner: Mutex::new(VmObjectInner {
                parent: Arc::downgrade(self),
                content_size: child_content_size,
                ..VmObjectInner::default()
            }),
        });
        self.add_child(&child);
        Ok(child)
    }

    /// Add child to the list and signal if ZeroChildren signal is active.
    /// If the number of children turns 0 to 1, signal it
    fn add_child(&self, child: &Arc<VmObject>) {
        let mut inner = self.inner.lock();
        inner.children.retain(|x| x.strong_count() != 0);
        inner.children.push(Arc::downgrade(child));
        if inner.children.len() == 1 {
            self.base.signal_clear(Signal::VMO_ZERO_CHILDREN);
        }
    }

    /// Set the length of this VMO if resizable.
    pub fn set_len(&self, len: usize) -> ZxResult {
        let size = roundup_pages(len);
        if size < len {
            return Err(ZxError::OUT_OF_RANGE);
        }
        if !self.resizable {
            // Slices/references return ACCESS_DENIED; regular
            // non-resizable VMOs return UNAVAILABLE.
            if self.is_slice {
                return Err(ZxError::ACCESS_DENIED);
            }
            return Err(ZxError::UNAVAILABLE);
        }
        self.trait_.set_len(size)
    }

    /// Set the size of the content stored in the VMO in bytes, resize vmo if needed
    pub fn set_content_size_and_resize(
        &self,
        size: usize,
        zero_until_offset: usize,
    ) -> ZxResult<usize> {
        let mut inner = self.inner.lock();
        let content_size = inner.content_size;
        let len = self.trait_.len();
        if size < content_size {
            return Ok(content_size);
        }
        let required_len = roundup_pages(size);
        let new_content_size = if required_len > len && self.set_len(required_len).is_err() {
            len
        } else {
            size
        };
        let zero_until_offset = zero_until_offset.min(new_content_size);
        if zero_until_offset > content_size {
            self.trait_
                .zero(content_size, zero_until_offset - content_size)?;
        }
        inner.content_size = new_content_size;
        Ok(new_content_size)
    }

    /// Get the size of the content stored in the VMO in bytes.
    pub fn content_size(&self) -> usize {
        let inner = self.inner.lock();
        inner.content_size
    }

    /// Get the size of the content stored in the VMO in bytes.
    /// Set content_size without zeroing.
    ///
    /// Used internally by stream write to extend the high-water mark.
    /// The caller is responsible for ensuring data beyond the new
    /// Mark this VMO as immutable (SNAPSHOT + NO_WRITE child).
    pub fn set_immutable(&self) {
        self.immutable
            .store(true, core::sync::atomic::Ordering::Relaxed);
    }

    /// content_size is properly initialized.
    pub fn set_content_size(&self, size: usize) -> ZxResult {
        let mut inner = self.inner.lock();
        inner.content_size = size;
        Ok(())
    }

    /// Set content_size and zero data beyond it.
    ///
    /// Used by `ZX_PROP_VMO_CONTENT_SIZE` set_property to maintain the
    /// invariant that data beyond content_size reads as zero. Zeros from
    /// the new content_size to the old content_size (or page boundary).
    pub fn set_content_size_with_zero(&self, size: usize) -> ZxResult {
        let mut inner = self.inner.lock();
        let old = inner.content_size;
        inner.content_size = size;
        drop(inner);
        let vmo_len = self.len();
        // Zero from the new content_size to the page-rounded old content_size
        // (or vmo boundary). This covers both shrinking and the case where
        // raw vmo.write() placed data beyond the old content_size.
        let zero_start = size.min(vmo_len);
        let old_page_end = ((old + PAGE_SIZE - 1) & !(PAGE_SIZE - 1)).min(vmo_len);
        // Always zero at least to the page boundary of the new content_size
        // to handle the "raw write then set_property" pattern.
        let new_page_end = ((size + PAGE_SIZE - 1) & !(PAGE_SIZE - 1)).min(vmo_len);
        let zero_end = old_page_end.max(new_page_end);
        if zero_end > zero_start {
            self.zero(zero_start, zero_end - zero_start)?;
        }
        Ok(())
    }

    /// Zero a range of bytes within the VMO.
    pub fn zero(&self, offset: usize, len: usize) -> ZxResult {
        self.trait_.zero(offset, len)
    }

    /// Get information of this VMO.
    pub fn get_info(&self) -> VmoInfo {
        let inner = self.inner.lock();
        let mut ret = VmoInfo {
            koid: self.base.id,
            name: {
                let mut arr = [0u8; 32];
                let name = self.base.name();
                let length = name.len().min(32);
                arr[..length].copy_from_slice(&name.as_bytes()[..length]);
                arr
            },
            size: self.trait_.len() as u64,
            parent_koid: inner.parent.upgrade().map(|p| p.id()).unwrap_or(0),
            num_children: inner.children.len() as u64,
            flags: {
                let mut f = VmoInfoFlags::empty();
                if self.resizable {
                    f |= VmoInfoFlags::RESIZABLE;
                }
                if self.immutable.load(core::sync::atomic::Ordering::Relaxed) {
                    f |= VmoInfoFlags::IMMUTABLE;
                }
                f
            },
            cache_policy: self.trait_.cache_policy() as u32,
            share_count: inner.mapping_count as u64,
            ..Default::default()
        };
        self.trait_.complete_info(&mut ret);
        ret
    }

    /// Set the cache policy.
    pub fn set_cache_policy(&self, policy: CachePolicy) -> ZxResult {
        let inner = self.inner.lock();
        if !inner.children.is_empty() {
            return Err(ZxError::BAD_STATE);
        }
        if inner.mapping_count != 0 {
            return Err(ZxError::BAD_STATE);
        }
        self.trait_.set_cache_policy(policy)
    }

    /// Append a mapping to the VMO's mapping list.
    pub fn append_mapping(&self, mapping: Weak<VmMapping>) {
        let mut inner = self.inner.lock();
        inner.mapping_count += 1;
        inner.mappings.push(mapping.clone());
        drop(inner);
        self.trait_.append_mapping(mapping);
    }

    /// Remove a mapping from the VMO's mapping list.
    pub fn remove_mapping(&self, mapping: Weak<VmMapping>) {
        let mut inner = self.inner.lock();
        inner.mapping_count -= 1;
        inner.mappings.retain(|m| !Weak::ptr_eq(m, &mapping));
        drop(inner);
        self.trait_.remove_mapping(mapping);
    }

    /// Returns an estimate of the number of unique VmAspaces that this object
    /// is mapped into.
    pub fn share_count(&self) -> usize {
        let inner = self.inner.lock();
        inner.mapping_count
    }

    /// Returns true if the object size can be changed.
    pub fn is_resizable(&self) -> bool {
        self.resizable
    }

    /// Called when all handles to this VMO are closed.
    /// For child VMOs (slices, references, COW snapshots), invalidates
    /// all page table entries so accesses through existing mappings
    /// fault instead of returning stale data.
    ///
    /// Regular (non-child) VMOs are NOT affected — it's valid in
    /// Fuchsia to close a VMO handle and keep using the mapping.
    pub fn on_zero_handles_impl(&self) {
        let is_child = self.inner.lock().parent.upgrade().is_some();
        if !is_child {
            return;
        }
        self.dead.store(true, core::sync::atomic::Ordering::Release);
        let inner = self.inner.lock();
        for mapping_weak in &inner.mappings {
            if let Some(mapping) = mapping_weak.upgrade() {
                mapping.unmap_all_pages();
            }
        }
    }

    pub fn is_dead(&self) -> bool {
        self.dead.load(core::sync::atomic::Ordering::Acquire)
    }

    pub fn is_contiguous(&self) -> bool {
        self.trait_.is_contiguous()
    }
}

impl Deref for VmObject {
    type Target = Arc<dyn VMObjectTrait>;

    fn deref(&self) -> &Self::Target {
        &self.trait_
    }
}

impl Drop for VmObject {
    fn drop(&mut self) {
        // Wake any threads blocked on pager faults before freeing.
        {
            let inner = self.inner.lock();
            if !inner.pager_waiters.is_empty() {
                drop(inner);
                self.fail_pager_requests(ZxError::BAD_STATE);
            }
        }
        // For slice/reference VMOs, invalidate all page table entries.
        // This ensures that after a REFERENCE child is destroyed,
        // mapped pages become inaccessible (reads return zeroes).
        if self.is_slice {
            let inner = self.inner.lock();
            for mapping_weak in &inner.mappings {
                if let Some(mapping) = mapping_weak.upgrade() {
                    mapping.unmap_all_pages();
                }
            }
        }
        let mut inner = self.inner.lock();
        let parent = match inner.parent.upgrade() {
            Some(parent) => parent,
            None => return,
        };
        for child in inner.children.iter() {
            if let Some(child) = child.upgrade() {
                child.inner.lock().parent = Arc::downgrade(&parent);
            }
        }
        let mut parent_inner = parent.inner.lock();
        let children = &mut parent_inner.children;
        children.append(&mut inner.children);
        children.retain(|c| c.strong_count() != 0);
        for child in children.iter() {
            let child = child.upgrade().unwrap();
            let mut inner = child.inner.lock();
            inner.children.retain(|c| c.strong_count() != 0);
            if inner.children.is_empty() {
                child.base.signal_set(Signal::VMO_ZERO_CHILDREN);
            }
        }
        // Non-zero to zero?
        if children.is_empty() {
            parent.base.signal_set(Signal::VMO_ZERO_CHILDREN);
        }
    }
}

/// Describes a VMO. Matches Fuchsia's `zx_info_vmo_t` layout (168 bytes).
#[repr(C)]
#[derive(Default)]
pub struct VmoInfo {
    /// The koid of this VMO.
    koid: KoID,
    /// The name of this VMO.
    name: [u8; 32],
    /// The size of this VMO; i.e., the amount of virtual address space it
    /// would consume if mapped.
    size: u64,
    /// If this VMO is a clone, the koid of its parent. Otherwise, zero.
    parent_koid: KoID,
    /// The number of clones of this VMO, if any.
    num_children: u64,
    /// The number of times this VMO is currently mapped into VMARs.
    num_mappings: u64,
    /// The number of unique address space we're mapped into.
    share_count: u64,
    /// Flags.
    pub flags: VmoInfoFlags,
    /// Padding.
    padding1: [u8; 4],
    /// If the type is `PAGED`, the amount of
    /// memory currently allocated to this VMO; i.e., the amount of physical
    /// memory it consumes. Undefined otherwise.
    pub committed_bytes: u64,
    /// If `flags & ZX_INFO_VMO_VIA_HANDLE`, the handle rights.
    /// Undefined otherwise.
    pub rights: Rights,
    /// VMO mapping cache policy.
    cache_policy: u32,
    /// Kernel memory used to track metadata for this VMO.
    metadata_bytes: u64,
    /// Running counter of committed-state change events.
    committed_change_events: u64,
    /// Content populated and tracked by this VMO (including shared pages).
    pub populated_bytes: u64,
    /// Physical memory allocated to only this VMO (not shared with clones).
    committed_private_bytes: u64,
    /// Content populated and tracked by only this VMO (not shared).
    populated_private_bytes: u64,
    /// `committed_bytes` scaled by sharing count (fractional bytes truncated).
    committed_scaled_bytes: u64,
    /// `populated_bytes` scaled by sharing count (fractional bytes truncated).
    populated_scaled_bytes: u64,
    /// Fractional remainder of `committed_scaled_bytes` (fixed-point, 63-bit precision).
    committed_fractional_scaled_bytes: u64,
    /// Fractional remainder of `populated_scaled_bytes` (fixed-point, 63-bit precision).
    /// Set to `u64::MAX` when fractional scaling is not supported.
    pub populated_fractional_scaled_bytes: u64,
}

bitflags! {
    #[derive(Default)]
    /// Values used by ZX_INFO_PROCESS_VMOS.
    pub struct VmoInfoFlags: u32 {
        /// The VMO points to a physical address range, and does not consume memory.
        /// Typically used to access memory-mapped hardware.
        /// Mutually exclusive with TYPE_PAGED.
        const TYPE_PHYSICAL = 0;

        #[allow(clippy::identity_op)]
        /// The VMO is backed by RAM, consuming memory.
        /// Mutually exclusive with TYPE_PHYSICAL.
        const TYPE_PAGED    = 1 << 0;

        /// The VMO is resizable.
        const RESIZABLE     = 1 << 1;

        /// The VMO is a child, and is a copy-on-write clone.
        const IS_COW_CLONE  = 1 << 2;

        /// When reading a list of VMOs pointed to by a process, indicates that the
        /// process has a handle to the VMO, which isn't necessarily mapped.
        const VIA_HANDLE    = 1 << 3;

        /// When reading a list of VMOs pointed to by a process, indicates that the
        /// process maps the VMO into a VMAR, but doesn't necessarily have a handle to
        /// the VMO.
        const VIA_MAPPING   = 1 << 4;

        /// The VMO is a pager owned VMO created by zx_pager_create_vmo or is
        /// a clone of a VMO with this flag set. Will only be set on VMOs with
        /// the ZX_INFO_VMO_TYPE_PAGED flag set.
        const PAGER_BACKED  = 1 << 5;

        /// The VMO is contiguous.
        const CONTIGUOUS    = 1 << 6;

        /// The VMO is discardable.
        const DISCARDABLE   = 1 << 7;

        /// The VMO is immutable (created with NO_WRITE + SNAPSHOT).
        const IMMUTABLE     = 1 << 8;
    }
}

/// Different operations that `range_change` can perform against any VmMappings that are found.
#[allow(dead_code)]
#[derive(PartialEq, Eq, Clone, Copy)]
pub(super) enum RangeChangeOp {
    Unmap,
    RemoveWrite,
}

#[cfg(test)]
mod tests {
    use super::*;

    pub fn read_write(vmo: &VmObject) {
        let mut buf = [0u8; 4];
        vmo.write(0, &[0, 1, 2, 3]).unwrap();
        vmo.read(0, &mut buf).unwrap();
        assert_eq!(&buf, &[0, 1, 2, 3]);
    }

    #[test]
    fn create_contiguous() {
        // Create a 2-page contiguous VMO
        let vmo = VmObject::new_contiguous(2, PAGE_SIZE_LOG2).unwrap();
        assert!(!vmo.resizable);

        // Should be readable/writable like any VMO
        read_write(&vmo);

        // Size should be 2 pages
        assert_eq!(vmo.len(), PAGE_SIZE * 2);

        // get_info should report the VMO as contiguous
        let info = vmo.get_info();
        assert!(info.flags.contains(VmoInfoFlags::CONTIGUOUS));
        assert!(!info.flags.contains(VmoInfoFlags::RESIZABLE));
    }
}
