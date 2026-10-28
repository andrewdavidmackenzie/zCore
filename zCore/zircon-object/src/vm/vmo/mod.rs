use {
    self::{paged::*, physical::*, slice::*},
    super::*,
    crate::object::*,
    crate::signal::{
        PacketPageRequest, PayloadRepr, Port, PortPacketRepr, ZX_PAGER_VMO_COMPLETE,
        ZX_PAGER_VMO_DIRTY, ZX_PAGER_VMO_READ,
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
// Public counters for kernel memory statistics (KmemStatsExtended).
// These use a custom declaration instead of kcounter! to be pub(crate).
#[used]
pub(crate) static VMO_PAGER_PAGES: crate::util::kcounter::Counter =
    crate::util::kcounter::Counter::new();
#[used]
pub(crate) static VMO_DISCARDABLE_LOCKED_PAGES: crate::util::kcounter::Counter =
    crate::util::kcounter::Counter::new();
#[used]
pub(crate) static VMO_DISCARDABLE_UNLOCKED_PAGES: crate::util::kcounter::Counter =
    crate::util::kcounter::Counter::new();

/// Page dirty-tracking states.
pub const PAGE_CLEAN: u8 = 0;
pub const PAGE_DIRTY: u8 = 1;
pub const PAGE_AWAITING_CLEAN: u8 = 2;
/// Zero page from resize — implicitly dirty (shows as IS_ZERO in
/// query_dirty_ranges). For TRAP_DIRTY, writes still trap (not
/// considered dirty for write-gating). Writeback transitions
/// PAGE_ZERO_DIRTY → PAGE_AWAITING_CLEAN → PAGE_CLEAN.
const PAGE_ZERO_DIRTY: u8 = 3;
/// Page beyond the stream_size boundary on an unbounded VMO.
/// Not reported by query_dirty_ranges. Writes are allowed but
/// the dirty state is not tracked. When stream_size increases
/// past this page, it transitions to PAGE_ZERO_DIRTY.
/// Matches Fuchsia's IntervalDirtyState::Untracked.
const PAGE_UNTRACKED: u8 = 4;

/// The amount of memory committed to VMOs.
pub fn vmo_page_bytes() -> usize {
    (VMO_PAGE_ALLOC.get() - VMO_PAGE_DEALLOC.get()) * PAGE_SIZE
}

/// Total bytes in pager-backed VMOs.
pub fn vmo_pager_total_bytes() -> usize {
    VMO_PAGER_PAGES.get() * PAGE_SIZE
}

/// Bytes in locked discardable VMOs.
pub fn vmo_discardable_locked_bytes() -> usize {
    VMO_DISCARDABLE_LOCKED_PAGES.get() * PAGE_SIZE
}

/// Bytes in unlocked discardable VMOs.
pub fn vmo_discardable_unlocked_bytes() -> usize {
    VMO_DISCARDABLE_UNLOCKED_PAGES.get() * PAGE_SIZE
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

    /// Decommit all pages in this VMO (for discardable VMO discard).
    fn decommit_all(&self) -> ZxResult {
        self.decommit(0, self.len())
    }

    /// Create a child VMO (bidirectional COW clone with hidden parent).
    fn create_child(&self, offset: usize, len: usize) -> ZxResult<Arc<dyn VMObjectTrait>>;

    /// Create a unidirectional child VMO (SNAPSHOT_MODIFIED on pager root).
    /// The child hangs directly under the parent; unmodified pages read through.
    fn create_child_unidirectional(
        &self,
        _offset: usize,
        _len: usize,
    ) -> ZxResult<Arc<dyn VMObjectTrait>> {
        Err(ZxError::NOT_SUPPORTED)
    }

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
    fn is_physical(&self) -> bool {
        false
    }
    fn is_reference(&self) -> bool {
        false
    }
    /// Offset into the parent VMO (for slices). 0 for non-slices.
    fn parent_offset(&self) -> usize {
        0
    }

    /// Returns true if the object is backed by RAM.
    fn is_paged(&self) -> bool {
        false
    }

    /// Whether this VMO is a COW child (SNAPSHOT or SNAPSHOT_AT_LEAST_ON_WRITE).
    fn is_cow_child(&self) -> bool {
        false
    }

    /// Whether the page at the given index is shared with the parent VMO.
    fn page_is_shared(&self, _page_idx: usize) -> bool {
        false
    }

    /// Mark this VMO as pager-backed. Pages will not be auto-committed
    /// by `commit_page` — they must be supplied via the pager.
    fn set_pager_backed(&self) {}

    /// Returns true if this VMO is pager-backed.
    fn is_pager_backed(&self) -> bool {
        false
    }

    /// Set the supplying flag — allows commit_page to allocate frames
    /// on pager-backed VMOs during supply_pages.
    fn set_supplying(&self, _v: bool) {}

    /// Insert a zero page marker at the given page index.
    /// Zero markers indicate that the page is logically zero but no
    /// physical frame is committed. Used during supply_pages when the
    /// source page is all zeros.
    fn insert_zero_marker(&self, _page_idx: usize) {}

    /// Check if a page has a zero marker.
    fn has_zero_marker(&self, _page_idx: usize) -> bool {
        false
    }

    /// Check if a specific page is pinned (has pin_count > 0).
    fn is_page_pinned(&self, _page_idx: usize) -> bool {
        false
    }

    /// Remove a zero marker at the given page index.
    fn remove_zero_marker(&self, _page_idx: usize) {}

    /// Decommit a single page, replacing it with a zero marker.
    /// Used during pager detach to reclaim clean page frames.
    fn decommit_page(&self, _page_idx: usize) {}

    /// Get the offset of this VMO within its parent (for slices).
    /// Returns 0 for non-slice VMOs.
    fn slice_offset(&self) -> usize {
        0
    }

    /// Unmap pages from all mappings (for writeback_end).
    fn range_change_unmap(&self, _page_idx: usize, _count: usize) {}

    /// Remove write permission from all mappings for a page range.
    /// Used by writeback_begin to force re-faults on subsequent writes.
    fn range_change_remove_write(&self, _page_idx: usize, _count: usize) {}

    /// Clear the pager-backed flag. Called when the pager is detached.
    fn clear_pager_backed(&self) {}

    /// Set the content boundary for unbounded VMOs (stream_size in pages).
    fn set_content_boundary(&self, _pages: usize) {}

    /// If contiguous, transmute vmo to a mutable buffer
    fn as_mut_buf(&self) -> ZxResult<(MutexGuard<'_, ()>, &mut [u8])> {
        Err(ZxError::NOT_SUPPORTED)
    }

    /// Mark as not contiguous
    fn unset_contiguous(&self) {}

    /// Check whether this VMO's cow_pages can create a unidirectional
    /// clone. Returns true if:
    /// 1. The root of the tree is pager-backed (user pager)
    /// 2. The cow_pages parent (if any) is not a hidden node
    ///
    /// Mirrors Fuchsia's `VmCowPages::can_unidirectional_clone_locked`.
    fn can_unidirectional_clone(&self) -> bool {
        false
    }
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
    discardable: core::sync::atomic::AtomicBool,
    /// True if a discardable VMO has been unlocked and its pages discarded.
    /// TRY_LOCK returns UNAVAILABLE when this is set.
    discarded: core::sync::atomic::AtomicBool,
    /// Lock count for discardable VMOs (0 = unlocked, >0 = locked).
    lock_count: core::sync::atomic::AtomicU32,
    /// True if all handles are closed (for slice VMOs, commit_page
    /// returns NOT_FOUND to prevent re-mapping parent data).
    dead: core::sync::atomic::AtomicBool,
    trait_: Arc<dyn VMObjectTrait>,
    inner: Mutex<VmObjectInner>,
}

impl_kobject!(VmObject
    fn on_zero_handles(&self) {
        self.on_zero_handles_impl();
    }
    fn as_vmo(&self) -> Option<&crate::vm::VmObject> {
        Some(self)
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
    /// KoID of the pager that created this VMO. Set during create_vmo,
    /// preserved after detach so writeback/query operations can verify
    /// the caller is the original pager.
    pager_koid: u64,
    /// Whether this VMO traps writes for dirty-page notifications.
    trap_dirty: bool,
    /// Whether this VMO is unbounded (ZX_VMO_UNBOUNDED). Unbounded VMOs
    /// have a large capacity but track content via stream_size
    /// (content_size). Pages beyond stream_size are PAGE_UNTRACKED.
    unbounded: bool,
    /// Per-page dirty state tracking.
    /// Clean = 0, Dirty = 1, AwaitingClean = 2
    /// Only used when `trap_dirty` is true.
    dirty_pages: Vec<u8>,
    /// Whether the VMO has been modified since last stats query.
    modified: bool,
    /// Threads waiting for pager to supply/dirty pages. Each entry is a
    /// (page_index, sender) pair. Waiters for both READ and DIRTY
    /// requests share this list — they're woken by supply_pages or
    /// op_range(DIRTY) respectively.
    pager_waiters: Vec<(usize, futures::channel::oneshot::Sender<ZxResult>)>,
    /// Pages that have been failed by OP_FAIL. Maps page_idx → error.
    /// When wait_for_page registers a waiter for a failed page, it
    /// returns the error immediately instead of blocking.
    pager_failed_pages: alloc::collections::BTreeMap<usize, ZxError>,
    /// Outstanding pager requests. Each entry represents a range sent
    /// to the pager port. New requests whose range falls within an
    /// existing outstanding request are absorbed (no new port packet
    /// generated) and attached as overlaps. When the primary request
    /// completes, all overlapping waiters are woken.
    ///
    /// This matches Fuchsia's `PageSource::outstanding_requests_` tree
    /// with per-request overlap lists.
    outstanding_requests: Vec<OutstandingPageRequest>,
}

/// An outstanding pager page request. Tracks the range sent to the
/// pager port and any overlapping waiters that were absorbed.
struct OutstandingPageRequest {
    /// Start page index of the request.
    start_page: usize,
    /// End page index (exclusive) of the request.
    end_page: usize,
    /// Number of pages not yet supplied/failed. When this reaches 0,
    /// the request is complete and all waiters are woken.
    pending_pages: usize,
    /// True if this is a DIRTY request, false for READ.
    is_dirty: bool,
    /// Overlapping page waiters attached to this request. Each has:
    /// - page index
    /// - oneshot sender to wake the waiter
    /// - per-waiter error status (set by FailPages, used at completion)
    overlap_waiters: Vec<OverlapWaiter>,
}

/// An individual overlap waiter within an outstanding request.
struct OverlapWaiter {
    page_idx: usize,
    sender: futures::channel::oneshot::Sender<ZxResult>,
    /// Per-waiter error status, matching Fuchsia's `complete_status_`.
    /// Set by `fail_pager_requests_range` when the waiter's page is
    /// failed. Used at completion to send the error to the waiter.
    error: Option<ZxError>,
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
            discardable: core::sync::atomic::AtomicBool::new(false),
            discarded: core::sync::atomic::AtomicBool::new(false),
            lock_count: core::sync::atomic::AtomicU32::new(0),
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
            discardable: core::sync::atomic::AtomicBool::new(false),
            discarded: core::sync::atomic::AtomicBool::new(false),
            lock_count: core::sync::atomic::AtomicU32::new(0),
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
            discardable: core::sync::atomic::AtomicBool::new(false),
            discarded: core::sync::atomic::AtomicBool::new(false),
            lock_count: core::sync::atomic::AtomicU32::new(0),
            dead: core::sync::atomic::AtomicBool::new(false),
            _counter: CountHelper::new(),
            trait_: VMObjectPaged::new_contiguous(pages, align_log2)?,
            inner: Mutex::new(VmObjectInner::default()),
        });
        Ok(vmo)
    }

    /// Returns true if this VMO is pager-backed.
    pub fn has_pager(&self) -> bool {
        self.inner.lock().pager_port.is_some()
    }

    /// Read from a pager-backed VMO with demand-paging support.
    ///
    /// For each page in the read range, if the page is not yet committed
    /// and this VMO has a pager, sends a `ZX_PAGER_VMO_READ` page request
    /// to the pager's port and waits for the page to be supplied before
    /// reading.
    ///
    /// For non-pager VMOs, this is equivalent to `vmo.read()`.
    pub async fn read_paged(&self, offset: usize, buf: &mut [u8]) -> ZxResult {
        if !self.has_pager() || buf.is_empty() {
            return self.read(offset, buf);
        }
        // Demand-page: request and wait for any missing pages in the range.
        let start_page = offset / PAGE_SIZE;
        let end_page = (offset + buf.len()).div_ceil(PAGE_SIZE);
        for page_idx in start_page..end_page {
            if self.committed_pages_in_range(page_idx, page_idx + 1) == 0 {
                let page_offset = page_idx * PAGE_SIZE;
                // request_pages sends ZX_PAGER_VMO_READ to the port.
                // Note: wait_for_page registers the waiter under the VMO
                // lock and re-checks committed status, so supply_pages
                // completing between these two calls is handled correctly.
                let _ = self.request_pages(page_offset, PAGE_SIZE);
                // Block until the pager calls supply_pages for this page.
                self.wait_for_page(page_offset).await?;
            }
        }
        // All pages should now be present.
        self.read(offset, buf)
    }

    /// Set the pager association for this VMO.
    ///
    /// When a page fault occurs on an uncommitted page, a
    /// `ZX_PAGER_VMO_READ` packet will be sent to the port.
    pub fn set_pager(&self, port: Arc<Port>, key: u64, pager_koid: u64) {
        let mut inner = self.inner.lock();
        inner.pager_port = Some(port);
        inner.pager_key = key;
        inner.pager_koid = pager_koid;
        // Mark the underlying paged VMO so commit_page won't
        // auto-allocate zero frames — pages must come from supply_pages.
        self.trait_.set_pager_backed();
    }

    /// Get the KoID of the pager that created this VMO.
    pub fn pager_koid(&self) -> u64 {
        self.inner.lock().pager_koid
    }

    /// Set the supplying flag on the underlying paged VMO.
    /// Called by `supply_pages` to allow `commit_page` to allocate
    /// frames on pager-backed VMOs.
    pub fn set_supplying(&self, v: bool) {
        self.trait_.set_supplying(v);
    }

    /// Insert a zero page marker at the given page index.
    pub fn insert_zero_marker(&self, page_idx: usize) {
        self.trait_.insert_zero_marker(page_idx);
    }

    /// Check if a page has a zero marker.
    pub fn has_zero_marker(&self, page_idx: usize) -> bool {
        self.trait_.has_zero_marker(page_idx)
    }

    /// Decommit pages that are clean (not dirty or awaiting-clean).
    /// Called after pager detach to reclaim physical memory for pages
    /// that haven't been modified.
    pub fn decommit_clean_pages(&self) {
        let inner = self.inner.lock();
        let total_pages = self.len() / PAGE_SIZE;
        let mut any_decommitted = false;
        for page_idx in 0..total_pages {
            let state = inner
                .dirty_pages
                .get(page_idx)
                .copied()
                .unwrap_or(PAGE_CLEAN);
            if state == PAGE_CLEAN {
                // Clean page — can be safely decommitted.
                // Insert a zero marker to prevent re-requesting from
                // the (now-detached) pager.
                self.trait_.decommit_page(page_idx);
                any_decommitted = true;
            }
        }
        // Invalidate all VMAR mappings for decommitted pages.
        // Without this, stale page table entries let threads read
        // zero data instead of faulting (which should deliver a
        // FatalPageFault exception after pager detach).
        if any_decommitted {
            for mapping in inner.mappings.iter() {
                if let Some(map) = mapping.upgrade() {
                    map.range_change(0, total_pages, RangeChangeOp::Unmap);
                }
            }
        }
    }

    /// Get the offset of this VMO within its parent (for slices).
    pub fn get_slice_offset(&self) -> usize {
        self.trait_.slice_offset()
    }

    /// Check if the given page has a committed physical frame.
    pub fn has_committed_page(&self, page_idx: usize) -> bool {
        self.trait_.committed_pages_in_range(page_idx, page_idx + 1) > 0
    }

    /// Check if a page is in the PAGE_ZERO_DIRTY state (from resize).
    pub fn is_page_zero_dirty(&self, page_idx: usize) -> bool {
        let inner = self.inner.lock();
        inner
            .dirty_pages
            .get(page_idx)
            .copied()
            .unwrap_or(PAGE_CLEAN)
            == PAGE_ZERO_DIRTY
    }

    /// Remove a zero marker at the given page index.
    pub fn remove_zero_marker(&self, page_idx: usize) {
        self.trait_.remove_zero_marker(page_idx);
    }

    /// Mark pages as dirty-zero (for OP_ZERO on TRAP_DIRTY VMOs).
    /// This doesn't generate a DIRTY request — it directly sets the
    /// dirty state since zeroing is allowed without pager ACK.
    pub fn mark_pages_zero_dirty(&self, start_page: usize, end_page: usize) {
        let mut inner = self.inner.lock();
        for page_idx in start_page..end_page.min(inner.dirty_pages.len()) {
            // Only mark as zero-dirty if the page has a zero marker
            // (was actually zeroed). If the page has a committed frame,
            // mark as regular dirty.
            if self.trait_.has_zero_marker(page_idx)
                && self.trait_.committed_pages_in_range(page_idx, page_idx + 1) == 0
            {
                inner.dirty_pages[page_idx] = PAGE_ZERO_DIRTY;
            } else {
                inner.dirty_pages[page_idx] = PAGE_DIRTY;
            }
        }
    }

    /// Enable dirty-page trapping for this pager-backed VMO.
    /// Writes to clean pages will send `ZX_PAGER_VMO_DIRTY` to the
    /// pager port and block until the pager responds.
    pub fn set_trap_dirty(&self, pages: usize) {
        let mut inner = self.inner.lock();
        inner.trap_dirty = true;
        inner.dirty_pages = alloc::vec![PAGE_CLEAN; pages];
    }

    /// Mark this VMO as unbounded. Pages beyond content_size
    /// (stream_size) are PAGE_UNTRACKED.
    pub fn set_unbounded(&self, total_pages: usize, stream_size_pages: usize) {
        let mut inner = self.inner.lock();
        inner.unbounded = true;
        // Initialize dirty_pages: pages within stream_size are CLEAN,
        // pages beyond are UNTRACKED.
        inner.dirty_pages = alloc::vec![PAGE_UNTRACKED; total_pages];
        for state in inner.dirty_pages[..stream_size_pages.min(total_pages)].iter_mut() {
            *state = PAGE_CLEAN;
        }
        drop(inner);
        // Set the content boundary on the paged inner so commit_page
        // returns zeros for pages beyond stream_size without pager requests.
        self.trait_.set_content_boundary(stream_size_pages);
    }

    /// Check if this VMO is unbounded.
    pub fn is_unbounded(&self) -> bool {
        self.inner.lock().unbounded
    }

    /// Update dirty tracking when stream_size changes on an unbounded VMO.
    /// Matches Fuchsia's behavior where pages beyond stream_size are
    /// IntervalDirtyState::Untracked.
    pub fn update_stream_size_dirty_tracking(&self, old_size: usize, new_size: usize) {
        let mut inner = self.inner.lock();
        let old_pages = old_size.div_ceil(PAGE_SIZE);
        let new_pages = new_size.div_ceil(PAGE_SIZE);
        if new_pages > old_pages {
            // Increase: [old_pages, new_pages) → ZERO_DIRTY
            // These pages are now within the tracked stream_size range.
            let mut pages_to_decommit = alloc::vec::Vec::new();
            // Transition [old, new) from UNTRACKED/DIRTY → ZERO_DIRTY
            for pg in old_pages..new_pages.min(inner.dirty_pages.len()) {
                let old_state = inner.dirty_pages[pg];
                inner.dirty_pages[pg] = PAGE_ZERO_DIRTY;
                if old_state == PAGE_DIRTY {
                    pages_to_decommit.push(pg);
                }
            }
            // Pages beyond new stream_size that were committed while
            // untracked should be absorbed back to UNTRACKED. Their
            // frames are decommitted. In Fuchsia, these become
            // "untracked zero intervals".
            for pg in new_pages..inner.dirty_pages.len() {
                if inner.dirty_pages[pg] == PAGE_DIRTY {
                    inner.dirty_pages[pg] = PAGE_UNTRACKED;
                    pages_to_decommit.push(pg);
                }
            }
            inner.modified = true;
            drop(inner);
            for pg in pages_to_decommit {
                self.trait_.decommit_page(pg);
                // Keep the zero marker set by decommit_page so that
                // reads return zeros without requesting from the pager.
                // Previously, remove_zero_marker was called here which
                // caused VMO reads to block forever waiting for a pager
                // supply that would never come (the page is a dirty
                // zero, not a clean page needing supply).
            }
        } else if new_pages < old_pages {
            // Decrease: [new_pages, old_pages) → UNTRACKED
            // These pages are now beyond the tracked stream_size.
            for pg in new_pages..old_pages.min(inner.dirty_pages.len()) {
                inner.dirty_pages[pg] = PAGE_UNTRACKED;
            }
            inner.modified = true;
            drop(inner);
        } else {
            drop(inner);
        }
        // Update the content boundary on the paged inner.
        self.trait_.set_content_boundary(new_pages);
    }

    /// Ensure dirty-page tracking is initialized for this VMO.
    /// Called during supply_pages for all pager-backed VMOs. For
    /// non-TRAP_DIRTY VMOs, this allows query_dirty_ranges to work
    /// (writes auto-mark pages as dirty).
    pub fn ensure_dirty_tracking(&self, pages: usize) {
        let mut inner = self.inner.lock();
        if inner.dirty_pages.is_empty() {
            inner.dirty_pages = alloc::vec![PAGE_CLEAN; pages];
        } else if inner.dirty_pages.len() < pages {
            inner.dirty_pages.resize(pages, PAGE_CLEAN);
        }
    }

    /// Mark a page as dirty (for non-TRAP_DIRTY auto-dirty tracking).
    /// Called when a page is written in a pager-backed VMO without
    /// TRAP_DIRTY enabled.
    pub fn auto_mark_dirty(&self, page_idx: usize) {
        let mut inner = self.inner.lock();
        if !inner.trap_dirty && page_idx < inner.dirty_pages.len() {
            inner.dirty_pages[page_idx] = PAGE_DIRTY;
            inner.modified = true;
        }
    }

    /// Get the dirty tracking state of a page, or None if out of bounds.
    pub fn get_page_dirty_state(&self, page_idx: usize) -> Option<u8> {
        let inner = self.inner.lock();
        inner.dirty_pages.get(page_idx).copied()
    }

    /// Check if a page is covered by an outstanding DIRTY pager request.
    pub fn is_page_in_outstanding_request(&self, page_idx: usize) -> bool {
        let inner = self.inner.lock();
        inner
            .outstanding_requests
            .iter()
            .any(|req| req.is_dirty && page_idx >= req.start_page && page_idx < req.end_page)
    }

    /// Mark a page as dirty without setting the modified flag.
    /// Used by pager_preflight_write to set dirty state during page
    /// lookup (before data copy), matching Fuchsia's RequirePage which
    /// marks dirty but defers modified to after the copy succeeds.
    pub fn mark_page_dirty_only(&self, page_idx: usize) {
        let mut inner = self.inner.lock();
        if page_idx < inner.dirty_pages.len() {
            inner.dirty_pages[page_idx] = PAGE_DIRTY;
        }
    }

    /// Check if this VMO has dirty-page trapping enabled.
    pub fn is_trap_dirty(&self) -> bool {
        self.inner.lock().trap_dirty
    }

    /// Send a dirty-page request to the pager for the given range.
    /// Returns `Err(SHOULD_WAIT)` — caller should await `wait_for_page`.
    pub fn request_dirty(&self, offset: usize, length: usize) -> ZxResult {
        let mut inner = self.inner.lock();
        let start_page = offset / PAGE_SIZE;
        let end_page = (offset + length).div_ceil(PAGE_SIZE);
        // Clear any stale failed-page markers in this range.
        for pg in start_page..end_page {
            inner.pager_failed_pages.remove(&pg);
        }
        let port = inner.pager_port.clone();
        if let Some(port) = port {
            // Check if this request is fully absorbed by an existing
            // outstanding DIRTY request. Only dirty requests absorb
            // other dirty requests — READ requests don't absorb DIRTY.
            let is_absorbed = inner.outstanding_requests.iter().any(|req| {
                req.is_dirty && start_page >= req.start_page && end_page <= req.end_page
            });
            if !is_absorbed {
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
                // Track as outstanding DIRTY request for overlap detection.
                inner.outstanding_requests.push(OutstandingPageRequest {
                    start_page,
                    end_page,
                    pending_pages: end_page - start_page,
                    is_dirty: true,
                    overlap_waiters: alloc::vec![],
                });
            }
            Err(ZxError::SHOULD_WAIT)
        } else {
            Err(ZxError::BAD_STATE)
        }
    }

    /// Mark pages as dirty (called by pager_op_range(DIRTY)).
    /// Wakes any threads waiting for dirty permission on these pages.
    /// Returns NOT_FOUND if any page in the range is not "present":
    /// - Committed pages (have a frame) are always present.
    /// - Zero-marker pages from resize are present only if they are
    ///   still in the dirty-zero state (PAGE_ZERO_DIRTY). After
    ///   writeback cleans them (PAGE_CLEAN), they become "unsupplied"
    ///   and OP_DIRTY returns NOT_FOUND.
    pub fn mark_pages_dirty(&self, offset: usize, length: usize) -> ZxResult {
        let start_page = offset / PAGE_SIZE;
        let end_page = (offset + length).div_ceil(PAGE_SIZE);
        // Two-pass: first validate ALL pages are present (committed
        // or zero-marker), then allocate frames for zero-marker pages.
        // This avoids partial allocation if a later page is NOT_FOUND.
        if length > 0 {
            // Pass 1: validate
            for page_idx in start_page..end_page {
                let has_frame = self.trait_.committed_pages_in_range(page_idx, page_idx + 1) > 0;
                if has_frame {
                    continue;
                }
                if !self.trait_.has_zero_marker(page_idx) {
                    // Page was cleaned by WritebackEnd (no frame, no
                    // marker). Wake any kernel threads waiting for
                    // dirty on pages in this range so they can retry
                    // (re-request READ then DIRTY). Without this,
                    // threads blocked in wait_for_dirty hang forever.
                    let mut inner = self.inner.lock();
                    let mut i = 0;
                    while i < inner.pager_waiters.len() {
                        if inner.pager_waiters[i].0 >= start_page
                            && inner.pager_waiters[i].0 < end_page
                        {
                            let (_, tx) = inner.pager_waiters.swap_remove(i);
                            let _ = tx.send(Err(ZxError::NOT_FOUND));
                        } else {
                            i += 1;
                        }
                    }
                    // Remove outstanding requests covering this range
                    // so retries generate new port packets.
                    let mut req_idx = 0;
                    while req_idx < inner.outstanding_requests.len() {
                        let req = &inner.outstanding_requests[req_idx];
                        if req.start_page < end_page && req.end_page > start_page {
                            let removed = inner.outstanding_requests.swap_remove(req_idx);
                            for waiter in removed.overlap_waiters {
                                let _ = waiter.sender.send(Err(ZxError::NOT_FOUND));
                            }
                            continue;
                        }
                        req_idx += 1;
                    }
                    return Err(ZxError::NOT_FOUND);
                }
            }
            // Pass 2: allocate frames for zero-marker pages
            for page_idx in start_page..end_page {
                if self.trait_.committed_pages_in_range(page_idx, page_idx + 1) > 0 {
                    continue;
                }
                // Zero-marker page: allocate a real frame.
                self.trait_.commit_page(page_idx, MMUFlags::WRITE)?;
            }
        }
        // Second pass: update dirty state under inner lock.
        let mut inner = self.inner.lock();
        for page_idx in start_page..end_page.min(inner.dirty_pages.len()) {
            inner.dirty_pages[page_idx] = PAGE_DIRTY;
        }
        // Clear any failed-page markers in the dirtied range.
        // A successful OP_DIRTY supersedes a previous OP_FAIL for
        // these pages. Without this, stale failure markers cause
        // subsequent wait_for_dirty to return the old error.
        for page_idx in start_page..end_page {
            inner.pager_failed_pages.remove(&page_idx);
        }
        // Note: OP_DIRTY does NOT set the modified flag. Only actual
        // writes to the VMO (vmo_write, mapping write) set modified.
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
        // Resolve outstanding dirty requests that overlap this range.
        // Decrement pending_pages for dirtied pages. When a request
        // reaches 0 pending, wake all overlap waiters and remove it.
        let mut req_idx = 0;
        while req_idx < inner.outstanding_requests.len() {
            let req = &mut inner.outstanding_requests[req_idx];
            let overlap_start = start_page.max(req.start_page);
            let overlap_end = end_page.min(req.end_page);
            if overlap_start < overlap_end {
                let newly_dirtied = overlap_end - overlap_start;
                req.pending_pages = req.pending_pages.saturating_sub(newly_dirtied);
                if req.pending_pages == 0 {
                    let completed = inner.outstanding_requests.swap_remove(req_idx);
                    for waiter in completed.overlap_waiters {
                        let _ = waiter.sender.send(Ok(()));
                    }
                    continue; // don't increment — swap_remove shifted
                }
            }
            req_idx += 1;
        }
        Ok(())
    }

    /// Check if a page is dirty (i.e., write-allowed for TRAP_DIRTY VMOs).
    pub fn is_page_dirty(&self, page_idx: usize) -> bool {
        let inner = self.inner.lock();
        if !inner.trap_dirty {
            return true; // Non-TRAP_DIRTY VMOs treat all pages as writable
        }
        // PAGE_DIRTY and PAGE_ZERO_DIRTY are both considered dirty
        // for write-permission purposes. PAGE_AWAITING_CLEAN pages
        // need another DIRTY acknowledgement before writing, matching
        // Fuchsia's RequirePage which treats AwaitingClean like Clean.
        matches!(
            inner.dirty_pages.get(page_idx).copied(),
            Some(PAGE_DIRTY) | Some(PAGE_ZERO_DIRTY)
        )
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
        for (i, &state) in inner.dirty_pages[start_page..max_page].iter().enumerate() {
            if state == PAGE_CLEAN {
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

    /// Check if this VMO is pager-backed (has an active pager).
    pub fn is_pager_backed(&self) -> bool {
        self.inner.lock().pager_port.is_some()
    }

    /// Check if this VMO was created as a pager-backed VMO.
    /// Returns true even after the pager is detached.
    pub fn was_pager_backed(&self) -> bool {
        self.trait_.is_pager_backed()
    }

    /// Check if this VMO or any ancestor in its clone tree is pager-backed.
    /// SNAPSHOT is not allowed on any VMO in a pager-backed tree.
    pub fn is_in_pager_tree(&self) -> bool {
        if self.has_pager() {
            return true;
        }
        let mut cur = self.inner.lock().parent.upgrade();
        while let Some(parent) = cur {
            if parent.has_pager() {
                return true;
            }
            cur = parent.inner.lock().parent.upgrade();
        }
        false
    }

    /// Find the root pager VMO in this VMO's ancestor chain.
    /// Returns the ancestor VmObject that has pager_port set.
    pub fn find_root_pager_vmo(&self) -> Option<Arc<VmObject>> {
        self.find_root_pager_vmo_with_offset().map(|(vmo, _)| vmo)
    }

    /// Walk up the parent chain to find the root pager-backed VMO,
    /// accumulating slice offsets along the way.  Returns `None` if
    /// this VMO itself is the pager VMO (has its own pager_port) or
    /// if no ancestor has a pager_port.
    pub fn find_root_pager_vmo_with_offset(&self) -> Option<(Arc<VmObject>, usize)> {
        let inner = self.inner.lock();
        if inner.pager_port.is_some() {
            return None; // this VMO IS the pager VMO
        }
        let mut cumulative_offset = self.get_slice_offset();
        let mut cur = inner.parent.upgrade()?;
        loop {
            let cur_inner = cur.inner.lock();
            if cur_inner.pager_port.is_some() {
                drop(cur_inner);
                return Some((cur, cumulative_offset));
            }
            cumulative_offset += cur.get_slice_offset();
            let next = cur_inner.parent.upgrade();
            drop(cur_inner);
            cur = next?;
        }
    }

    /// Query dirty page ranges within [offset, offset+length).
    /// Returns a list of (offset, length, options) tuples.
    ///
    /// Dirty states: PAGE_DIRTY, PAGE_AWAITING_CLEAN, PAGE_ZERO_DIRTY.
    /// Committed dirty pages → options=0.
    /// Zero-marker or uncommitted dirty pages → options=IS_ZERO (1).
    pub fn query_dirty_ranges(&self, offset: usize, length: usize) -> Vec<(u64, u64, u64)> {
        const ZX_VMO_DIRTY_RANGE_IS_ZERO: u64 = 1;
        let (start_page, max_page, dirty_snapshot) = {
            let inner = self.inner.lock();
            let start_page = offset / PAGE_SIZE;
            let end_page = (offset + length).div_ceil(PAGE_SIZE);
            let max_page = end_page.min(inner.dirty_pages.len());
            if start_page >= max_page {
                return Vec::new();
            }
            (
                start_page,
                max_page,
                inner.dirty_pages[start_page..max_page].to_vec(),
            )
        };
        let mut ranges = Vec::new();
        let mut run_start: Option<(usize, u64)> = None;
        for (i, &state) in dirty_snapshot.iter().enumerate() {
            let page_idx = start_page + i;
            let is_dirty =
                state == PAGE_DIRTY || state == PAGE_AWAITING_CLEAN || state == PAGE_ZERO_DIRTY;
            if is_dirty {
                let is_committed = self.trait_.committed_pages_in_range(page_idx, page_idx + 1) > 0;
                let has_zero = self.trait_.has_zero_marker(page_idx);
                let opts = if is_committed && !has_zero {
                    0u64
                } else {
                    ZX_VMO_DIRTY_RANGE_IS_ZERO
                };
                match run_start {
                    Some((_, run_opts)) if run_opts == opts => {}
                    Some((start, run_opts)) => {
                        ranges.push((
                            (start * PAGE_SIZE) as u64,
                            ((page_idx - start) * PAGE_SIZE) as u64,
                            run_opts,
                        ));
                        run_start = Some((page_idx, opts));
                    }
                    None => {
                        run_start = Some((page_idx, opts));
                    }
                }
            } else if let Some((start, run_opts)) = run_start {
                ranges.push((
                    (start * PAGE_SIZE) as u64,
                    ((page_idx - start) * PAGE_SIZE) as u64,
                    run_opts,
                ));
                run_start = None;
            }
        }
        if let Some((start, run_opts)) = run_start {
            ranges.push((
                (start * PAGE_SIZE) as u64,
                ((max_page - start) * PAGE_SIZE) as u64,
                run_opts,
            ));
        }
        ranges
    }

    /// Begin writeback: transition dirty pages in range to AwaitingClean.
    /// If `data` is `ZX_VMO_DIRTY_RANGE_IS_ZERO` (1), only transition
    /// PAGE_ZERO_DIRTY pages (skipping committed dirty pages). This
    /// allows the pager to writeback only the zero portions of a range.
    pub fn writeback_begin(&self, offset: usize, length: usize, data: u64) {
        const ZX_VMO_DIRTY_RANGE_IS_ZERO: u64 = 1;
        let zero_only = data == ZX_VMO_DIRTY_RANGE_IS_ZERO;
        let start_page = offset / PAGE_SIZE;
        let end_page = (offset + length).div_ceil(PAGE_SIZE);
        let page_count = end_page - start_page;
        // Remove write permission from all mappings FIRST, before
        // transitioning dirty state. This matches Fuchsia's ordering:
        // 1. Remove write perms → writes now fault
        // 2. Transition DIRTY → AWAITING_CLEAN
        // If a write fault occurs between (1) and (2), the page is
        // still DIRTY, so is_page_dirty() returns true and the fault
        // handler maps with WRITE (no dirty request). After (2),
        // subsequent faults see AWAITING_CLEAN and generate a new
        // DIRTY request. This eliminates the race where a write
        // sneaks through with WRITE-permitted PTEs after dirty state
        // has already been transitioned.
        self.trait_
            .range_change_remove_write(start_page, page_count);
        let mut inner = self.inner.lock();
        for page_idx in start_page..end_page.min(inner.dirty_pages.len()) {
            let state = inner.dirty_pages[page_idx];
            // When zero_only, skip committed dirty pages (PAGE_DIRTY).
            if zero_only && state == PAGE_DIRTY {
                continue;
            }
            if state == PAGE_DIRTY || state == PAGE_ZERO_DIRTY {
                // Skip pinned pages — they can't be cleaned while
                // a device has active DMA access.
                if self.trait_.is_page_pinned(page_idx) {
                    continue;
                }
                inner.dirty_pages[page_idx] = PAGE_AWAITING_CLEAN;
            }
        }
    }

    /// End writeback: transition AwaitingClean pages in range to Clean.
    /// Pages that were re-dirtied during writeback stay dirty.
    ///
    /// Matching Fuchsia's `WritebackEndLocked`:
    /// - Committed pages: dirty state transitions AwaitingClean → Clean.
    ///   Frames stay in memory (not decommitted). Write permissions
    ///   were already removed by `writeback_begin`.
    /// - Zero-marker pages: dirty state transitions + zero marker
    ///   removed. Pages become "empty" — subsequent writes need
    ///   READ+DIRTY from the pager. Existing READ PTEs remain valid.
    pub fn writeback_end(&self, offset: usize, length: usize) {
        let mut inner = self.inner.lock();
        let start_page = offset / PAGE_SIZE;
        let end_page = (offset + length).div_ceil(PAGE_SIZE);
        for page_idx in start_page..end_page.min(inner.dirty_pages.len()) {
            if inner.dirty_pages[page_idx] == PAGE_AWAITING_CLEAN {
                inner.dirty_pages[page_idx] = PAGE_CLEAN;
                // Remove zero marker atomically with dirty state
                // transition. Matches Fuchsia's WritebackEndLocked
                // which removes dirty zero intervals in the same
                // locked traversal. Pages become "empty" — subsequent
                // writes need READ+DIRTY. Committed frames stay.
                if self.trait_.has_zero_marker(page_idx) {
                    self.trait_.remove_zero_marker(page_idx);
                }
            }
        }
    }

    /// Query and optionally reset the modified flag.
    pub fn query_modified(&self, reset: bool) -> bool {
        let mut inner = self.inner.lock();
        let was_modified = inner.modified;
        if reset {
            inner.modified = false;
        }
        was_modified
    }

    /// Set the modified flag (called on write/resize/op_zero).
    pub fn set_modified(&self) {
        self.inner.lock().modified = true;
    }

    /// Clear the pager association (called on detach).
    pub fn clear_pager(&self) {
        let mut inner = self.inner.lock();
        inner.pager_port = None;
        inner.pager_key = 0;
        // Keep pager_backed flag set so unsupplied pages return
        // BAD_STATE instead of silently allocating zero frames.
        // The VMO was pager-backed; after detach it's in a
        // "detached" state where uncommitted pages fail.
        // Note: clear_pager_backed() is intentionally a no-op.
    }

    /// Send a page request to the pager for the given offset/length.
    /// Returns `Err(SHOULD_WAIT)` — the caller should then call
    /// `wait_for_page` to block until the page is supplied.
    pub fn request_pages(&self, offset: usize, length: usize) -> ZxResult {
        let mut inner = self.inner.lock();
        if let Some(port) = &inner.pager_port {
            let start_page = offset / PAGE_SIZE;
            let end_page = (offset + length).div_ceil(PAGE_SIZE);
            // Check if ALL pages are already committed.
            let committed = self.trait_.committed_pages_in_range(start_page, end_page);
            if committed >= end_page - start_page {
                return Err(ZxError::SHOULD_WAIT);
            }
            // Check if the request range falls within an existing
            // outstanding request (overlap absorption). Matching
            // Fuchsia's SendRequestToProviderLocked: if the new
            // request's offset is within an existing request, it is
            // absorbed — no new port packet is sent.
            let is_absorbed = inner
                .outstanding_requests
                .iter()
                .any(|req| start_page >= req.start_page && start_page < req.end_page);
            if !is_absorbed {
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
                // Track as a new outstanding request.
                let pending = (start_page..end_page)
                    .filter(|&pg| self.trait_.committed_pages_in_range(pg, pg + 1) == 0)
                    .count();
                inner.outstanding_requests.push(OutstandingPageRequest {
                    start_page,
                    end_page,
                    pending_pages: pending,
                    is_dirty: false,
                    overlap_waiters: Vec::new(),
                });
                info!(
                    "pager: requested page at offset={:#x} len={:#x}",
                    offset, length
                );
            }
            Err(ZxError::SHOULD_WAIT)
        } else if self.trait_.is_pager_backed() {
            // VMO was pager-backed but detached — unsupplied pages
            // should fail with BAD_STATE, not silently allocate zeros.
            Err(ZxError::BAD_STATE)
        } else {
            Err(ZxError::NOT_FOUND)
        }
    }

    /// Remove outstanding request entries covering the given range.
    /// Called when a pager wait is aborted (e.g., by suspend) and the
    /// caller will re-request. Without this, the stale entry causes
    /// overlap absorption to suppress the re-request's port packet.
    pub fn cancel_outstanding_requests_in_range(&self, offset: usize, length: usize) {
        let mut inner = self.inner.lock();
        let start_page = offset / PAGE_SIZE;
        let end_page = (offset + length).div_ceil(PAGE_SIZE);
        let mut i = 0;
        while i < inner.outstanding_requests.len() {
            let req = &inner.outstanding_requests[i];
            let overlaps = req.start_page < end_page && req.end_page > start_page;
            if overlaps {
                let removed = inner.outstanding_requests.swap_remove(i);
                // Wake any remaining overlap waiters with SHOULD_WAIT
                // so they can re-request rather than silently failing.
                for w in removed.overlap_waiters {
                    let _ = w.sender.send(Err(ZxError::SHOULD_WAIT));
                }
            } else {
                i += 1;
            }
        }
    }

    /// Check if a page has a pending waiter (another thread waiting for supply).
    pub fn has_page_waiter(&self, page_idx: usize) -> bool {
        let inner = self.inner.lock();
        inner.pager_waiters.iter().any(|(idx, _)| *idx == page_idx)
            || inner
                .outstanding_requests
                .iter()
                .any(|req| page_idx >= req.start_page && page_idx < req.end_page)
    }

    /// Clear a failed-page marker (for retry after OP_FAIL).
    /// Get the error for a failed page, if any.
    pub fn get_failed_page(&self, page_idx: usize) -> Option<ZxError> {
        self.inner.lock().pager_failed_pages.get(&page_idx).copied()
    }

    pub fn clear_failed_page(&self, page_idx: usize) {
        self.inner.lock().pager_failed_pages.remove(&page_idx);
    }

    /// Block until the page at `offset` is supplied by the pager.
    /// Returns `Ok(())` when the page is ready, or `Err` if the
    /// pager is detached.
    pub async fn wait_for_page(&self, offset: usize) -> ZxResult {
        let rx = {
            let mut inner = self.inner.lock();
            let page_idx = offset / PAGE_SIZE;
            // If the page is beyond the VMO's current size (e.g. after
            // resize), return OUT_OF_RANGE immediately.
            if offset >= self.len() {
                return Err(ZxError::OUT_OF_RANGE);
            }
            // If the page was already supplied between request_pages()
            // and now (race window), return immediately. Check both
            // committed frames and zero markers (supply_pages creates
            // zero markers for all-zero source pages).
            if self.committed_pages_in_range(page_idx, page_idx + 1) > 0
                || self.trait_.has_zero_marker(page_idx)
            {
                return Ok(());
            }
            // If the page was failed by OP_FAIL, return the error
            // immediately (handles race where fail arrives before waiter).
            if let Some(&err) = inner.pager_failed_pages.get(&page_idx) {
                return Err(err);
            }
            let (tx, rx) = futures::channel::oneshot::channel();
            // If the page falls within an outstanding request, add as
            // an overlap waiter (won't generate a new port packet).
            // Otherwise add to the general pager_waiters list.
            let absorbed = inner
                .outstanding_requests
                .iter_mut()
                .find(|req| page_idx >= req.start_page && page_idx < req.end_page);
            if let Some(req) = absorbed {
                req.overlap_waiters.push(OverlapWaiter {
                    page_idx,
                    sender: tx,
                    error: None,
                });
            } else {
                inner.pager_waiters.push((page_idx, tx));
            }
            rx
        };
        match rx.await {
            Ok(result) => result,
            Err(_) => Err(ZxError::CANCELED), // sender dropped
        }
    }

    /// Block until the page at `offset` is marked dirty by the pager.
    /// Used for TRAP_DIRTY VMOs when writing to a clean page.
    pub async fn wait_for_dirty(&self, offset: usize) -> ZxResult {
        let rx = {
            let mut inner = self.inner.lock();
            let page_idx = offset / PAGE_SIZE;
            // Already dirty — return immediately.
            if page_idx < inner.dirty_pages.len() && inner.dirty_pages[page_idx] == PAGE_DIRTY {
                return Ok(());
            }
            // If the page was failed, return the error.
            if let Some(&err) = inner.pager_failed_pages.get(&page_idx) {
                return Err(err);
            }
            let (tx, rx) = futures::channel::oneshot::channel();
            inner.pager_waiters.push((page_idx, tx));
            rx
        };
        match rx.await {
            Ok(result) => result,
            Err(_) => Err(ZxError::CANCELED),
        }
    }

    /// Wake threads waiting for pages in the given range.
    /// Called by `Pager::supply_pages` after writing page data.
    pub fn complete_pager_requests(&self, offset: usize, length: usize) {
        let mut inner = self.inner.lock();
        let start_page = offset / PAGE_SIZE;
        let end_page = (offset + length).div_ceil(PAGE_SIZE);
        // Clear any failed-page markers in the supplied range
        // (supply overrides a previous fail).
        for page_idx in start_page..end_page {
            inner.pager_failed_pages.remove(&page_idx);
        }
        // Drain general waiters whose page index falls in range.
        let mut i = 0;
        while i < inner.pager_waiters.len() {
            if inner.pager_waiters[i].0 >= start_page && inner.pager_waiters[i].0 < end_page {
                let (_, tx) = inner.pager_waiters.swap_remove(i);
                let _ = tx.send(Ok(()));
            } else {
                i += 1;
            }
        }
        // Resolve outstanding requests: decrement pending pages and
        // wake overlap waiters for supplied pages. When an outstanding
        // request's pending_pages reaches 0, it is complete — all
        // remaining overlap waiters are woken with Ok(()).
        let mut req_idx = 0;
        while req_idx < inner.outstanding_requests.len() {
            let req = &mut inner.outstanding_requests[req_idx];
            // Check if any supplied pages fall within this request.
            let overlap_start = start_page.max(req.start_page);
            let overlap_end = end_page.min(req.end_page);
            if overlap_start < overlap_end {
                // Count newly-supplied pages in the overlap.
                let newly_supplied = overlap_end - overlap_start;
                req.pending_pages = req.pending_pages.saturating_sub(newly_supplied);
                // Wake overlap waiters for pages in the supplied range.
                let mut wi = 0;
                while wi < req.overlap_waiters.len() {
                    if req.overlap_waiters[wi].page_idx >= start_page
                        && req.overlap_waiters[wi].page_idx < end_page
                    {
                        let w = req.overlap_waiters.swap_remove(wi);
                        let _ = w.sender.send(Ok(()));
                    } else {
                        wi += 1;
                    }
                }
                // If the request is fully resolved, wake any remaining
                // overlap waiters using their per-waiter error status.
                if req.pending_pages == 0 {
                    let completed = inner.outstanding_requests.swap_remove(req_idx);
                    for w in completed.overlap_waiters {
                        if let Some(err) = w.error {
                            let _ = w.sender.send(Err(err));
                        } else if self
                            .trait_
                            .committed_pages_in_range(w.page_idx, w.page_idx + 1)
                            > 0
                        {
                            let _ = w.sender.send(Ok(()));
                        } else {
                            let _ = w.sender.send(Err(ZxError::SHOULD_WAIT));
                        }
                    }
                    continue; // don't increment req_idx
                }
            }
            req_idx += 1;
        }
    }

    /// Wake all pager waiters with an error (called on detach/destroy).
    pub fn fail_pager_requests(&self, err: ZxError) {
        let mut inner = self.inner.lock();
        for (_, tx) in inner.pager_waiters.drain(..) {
            let _ = tx.send(Err(err));
        }
        // Also fail all outstanding request overlap waiters.
        for req in inner.outstanding_requests.drain(..) {
            for w in req.overlap_waiters {
                let _ = w.sender.send(Err(err));
            }
        }
    }

    /// Wake pager waiters in a specific range with an error
    /// (called by pager_op_range(FAIL)).
    /// Also records the failure so future wait_for_page calls for
    /// these pages immediately return the error.
    pub fn fail_pager_requests_range(&self, offset: usize, length: usize, err: ZxError) {
        let mut inner = self.inner.lock();
        let start_page = offset / PAGE_SIZE;
        let end_page = (offset + length).div_ceil(PAGE_SIZE);
        // Record failed pages for future waiters.
        for page_idx in start_page..end_page {
            inner.pager_failed_pages.insert(page_idx, err);
        }
        // Wake existing general waiters in range.
        let mut i = 0;
        while i < inner.pager_waiters.len() {
            if inner.pager_waiters[i].0 >= start_page && inner.pager_waiters[i].0 < end_page {
                let (_, tx) = inner.pager_waiters.swap_remove(i);
                let _ = tx.send(Err(err));
            } else {
                i += 1;
            }
        }
        // Fail overlap waiters in outstanding requests that overlap
        // the failed range. Also decrement pending_pages for failed
        // pages and complete requests that reach 0.
        let mut req_idx = 0;
        while req_idx < inner.outstanding_requests.len() {
            let req = &mut inner.outstanding_requests[req_idx];
            let overlap_start = start_page.max(req.start_page);
            let overlap_end = end_page.min(req.end_page);
            if overlap_start < overlap_end {
                let newly_failed = overlap_end - overlap_start;
                req.pending_pages = req.pending_pages.saturating_sub(newly_failed);
                // Store the error on overlap waiters whose pages fall
                // in the failed range. Don't wake them yet — they're
                // woken when the primary request completes, matching
                // Fuchsia's complete_status_ mechanism.
                for w in req.overlap_waiters.iter_mut() {
                    if w.page_idx >= start_page && w.page_idx < end_page && w.error.is_none() {
                        w.error = Some(err);
                    }
                }
                if req.pending_pages == 0 {
                    let completed = inner.outstanding_requests.swap_remove(req_idx);
                    for w in completed.overlap_waiters {
                        if let Some(w_err) = w.error {
                            let _ = w.sender.send(Err(w_err));
                        } else if self
                            .trait_
                            .committed_pages_in_range(w.page_idx, w.page_idx + 1)
                            > 0
                        {
                            let _ = w.sender.send(Ok(()));
                        } else {
                            let _ = w.sender.send(Err(err));
                        }
                    }
                    continue;
                }
            }
            req_idx += 1;
        }
    }

    /// Create a child VMO.
    pub fn create_child(
        self: &Arc<Self>,
        resizable: bool,
        offset: usize,
        len: usize,
    ) -> ZxResult<Arc<Self>> {
        // Slices and references are transparent windows into a parent VMO.
        // Creating a COW child of a slice/reference redirects through
        // the parent VMO with the slice's offset added.
        if self.is_slice {
            let parent = self
                .inner
                .lock()
                .parent
                .upgrade()
                .ok_or(ZxError::BAD_STATE)?;
            if self.trait_.is_reference() {
                // REFERENCE children: redirect to parent.
                // SNAPSHOT (create_child) on pager-backed VMOs is NOT_SUPPORTED.
                // Only SNAPSHOT_AT_LEAST_ON_WRITE (create_child_unidirectional)
                // works for pager VMOs.
                if parent.has_pager() {
                    return Err(ZxError::NOT_SUPPORTED);
                }
                let slice_off = self.trait_.parent_offset();
                let adjusted = offset.checked_add(slice_off).ok_or(ZxError::OUT_OF_RANGE)?;
                return parent.create_child(resizable, adjusted, len);
            }
            // Regular slices: redirect for non-pager parents.
            if parent.has_pager() {
                return Err(ZxError::NOT_SUPPORTED);
            }
            let slice_off = self.trait_.parent_offset();
            let adjusted = offset.checked_add(slice_off).ok_or(ZxError::OUT_OF_RANGE)?;
            return parent.create_child(resizable, adjusted, len);
        }
        let base = KObjectBase::with_signal(Signal::VMO_ZERO_CHILDREN);
        base.set_name(&self.base.name());
        let trait_ = self.trait_.create_child(offset, len)?;
        let child = Arc::new(VmObject {
            base,
            resizable,
            is_slice: false,
            immutable: core::sync::atomic::AtomicBool::new(false), // Caller sets this after creation if needed
            discardable: core::sync::atomic::AtomicBool::new(false),
            discarded: core::sync::atomic::AtomicBool::new(false),
            lock_count: core::sync::atomic::AtomicU32::new(0),
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

    /// Create a unidirectional child VMO (SNAPSHOT_MODIFIED on pager root).
    ///
    /// The child hangs directly under the parent — no hidden node.
    /// Unmodified pages read through to the parent.
    pub fn create_child_unidirectional(
        self: &Arc<Self>,
        offset: usize,
        len: usize,
        resizable: bool,
    ) -> ZxResult<Arc<Self>> {
        // SLICE and REFERENCE children: redirect to the underlying parent VMO.
        // SNAPSHOT_AT_LEAST_ON_WRITE on a slice creates a unidirectional
        // child of the parent VMO with offset adjusted by the slice offset.
        // This matches Fuchsia where slices are transparent windows and
        // clones are always created on the parent's cow_pages.
        if self.is_slice {
            let parent = self
                .inner
                .lock()
                .parent
                .upgrade()
                .ok_or(ZxError::BAD_STATE)?;
            let slice_off = self.trait_.parent_offset();
            let adjusted = offset.checked_add(slice_off).ok_or(ZxError::OUT_OF_RANGE)?;
            let child = parent.create_child_unidirectional(adjusted, len, resizable)?;
            // Set the child's VmObject parent to the slice so that
            // parent_koid points to the slice (matching Fuchsia).
            child.set_parent_ref(self);
            return Ok(child);
        }
        let base = KObjectBase::with_signal(Signal::VMO_ZERO_CHILDREN);
        base.set_name(&self.base.name());
        let trait_ = self.trait_.create_child_unidirectional(offset, len)?;
        let child = Arc::new(VmObject {
            base,
            resizable,
            is_slice: false,
            immutable: core::sync::atomic::AtomicBool::new(false),
            discardable: core::sync::atomic::AtomicBool::new(false),
            discarded: core::sync::atomic::AtomicBool::new(false),
            lock_count: core::sync::atomic::AtomicU32::new(0),
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
        self.create_slice_inner(offset, p_size, false, false)
    }

    /// Create a REFERENCE child (full-VMO alias).
    ///
    /// Unlike regular slices, REFERENCE children are allowed on
    /// resizable parents. If `resizable` is true, the child can
    /// resize the parent (requires parent to be resizable).
    pub fn create_reference_slice(
        self: &Arc<Self>,
        offset: usize,
        p_size: usize,
        resizable: bool,
    ) -> ZxResult<Arc<Self>> {
        self.create_slice_inner(offset, p_size, true, resizable)
    }

    fn create_slice_inner(
        self: &Arc<Self>,
        offset: usize,
        p_size: usize,
        allow_resizable_parent: bool,
        resizable: bool,
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
        // Slices of regular paged VMOs get content_size equal to the slice
        // size. Slices of physical or contiguous VMOs get content_size = 0
        // since these VMOs don't track content (stream_size is always 0).
        // REFERENCE children share the parent's content_size via delegation.
        let child_content_size = if self.trait_.is_paged() && !self.trait_.is_contiguous() {
            size
        } else {
            0
        };
        let child = Arc::new(VmObject {
            base: KObjectBase::with(&self.base.name(), Signal::VMO_ZERO_CHILDREN),
            resizable,
            is_slice: true,
            immutable: core::sync::atomic::AtomicBool::new(false),
            discardable: core::sync::atomic::AtomicBool::new(false),
            discarded: core::sync::atomic::AtomicBool::new(false),
            lock_count: core::sync::atomic::AtomicU32::new(0),
            dead: core::sync::atomic::AtomicBool::new(false),
            _counter: CountHelper::new(),
            trait_: if allow_resizable_parent {
                VMObjectSlice::new_reference(self.trait_.clone(), offset, size)
            } else {
                VMObjectSlice::new(self.trait_.clone(), offset, size)
            },
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

    /// Maximum VMO size — derived from the user address-space limit.
    ///
    /// A VMO cannot be meaningfully larger than the virtual address
    /// space it could be mapped into.  `USER_ASPACE_SIZE` is
    /// `(1 << 47) - 4096 - USER_ASPACE_BASE` (~128 TB), which is
    /// the largest mapping any single process can hold.  Using this
    /// as the VMO cap rejects astronomically large resize requests
    /// with `ZX_ERR_NO_MEMORY`, matching Fuchsia behaviour.
    const MAX_SIZE: usize = super::vmar::USER_ASPACE_SIZE as usize;

    /// Set the length of this VMO if resizable.
    pub fn set_len(&self, len: usize) -> ZxResult {
        let size = roundup_pages(len);
        if size < len {
            return Err(ZxError::OUT_OF_RANGE);
        }
        if size > Self::MAX_SIZE {
            return Err(ZxError::NO_MEMORY);
        }
        if !self.resizable {
            // Slices/references return ACCESS_DENIED; regular
            // non-resizable VMOs return UNAVAILABLE.
            if self.is_slice {
                return Err(ZxError::ACCESS_DENIED);
            }
            return Err(ZxError::UNAVAILABLE);
        }
        let old_size = self.len();
        let old_pages = old_size / PAGE_SIZE;
        self.trait_.set_len(size)?;
        let new_pages = size / PAGE_SIZE;
        // When shrinking a pager-backed VMO, wake any waiters for
        // pages in the truncated range with OUT_OF_RANGE.
        if size < old_size {
            self.fail_pager_requests_range(size, old_size - size, ZxError::OUT_OF_RANGE);
        }
        // Update dirty page tracking for pager-backed VMOs on resize.
        // Use was_pager_backed (trait method, no inner lock) to avoid
        // deadlock when called from set_content_size_and_resize which
        // already holds inner.
        if self.was_pager_backed() && size != old_size {
            let mut inner = self.inner.lock();
            if new_pages > old_pages {
                // Resize up: new pages are implicitly dirty zeros.
                // PAGE_ZERO_DIRTY shows as IS_ZERO in query_dirty_ranges
                // but is NOT considered dirty for TRAP_DIRTY write-gating
                // (writes still generate dirty requests).
                // First ensure existing pages are tracked (PAGE_CLEAN).
                if inner.dirty_pages.len() < old_pages {
                    inner.dirty_pages.resize(old_pages, PAGE_CLEAN);
                }
                // Then add new zero-dirty pages.
                inner.dirty_pages.resize(new_pages, PAGE_ZERO_DIRTY);
                inner.modified = true;
                drop(inner);
                // Set zero markers for the new pages so the fault handler
                // recognizes them as "present but zero" and doesn't send
                // page-supply requests to the pager. The page is already
                // "logically present" (all zeros from resize). On first
                // write, the TRAP_DIRTY gate (is_page_dirty returns false
                // for PAGE_ZERO_DIRTY) will generate a DIRTY request and
                // the pager's OP_DIRTY response will allocate a frame.
                for pg in old_pages..new_pages {
                    self.trait_.insert_zero_marker(pg);
                }
            } else {
                // Resize down: truncate the dirty_pages vector.
                inner.dirty_pages.truncate(new_pages);
                inner.modified = true;
            }
        }
        Ok(())
    }

    /// Set the size of the content stored in the VMO in bytes, resize vmo if needed
    pub fn set_content_size_and_resize(
        &self,
        size: usize,
        zero_until_offset: usize,
    ) -> ZxResult<usize> {
        // REFERENCE children delegate to the parent.
        // Acquire parent Arc first, drop the guard, then call —
        // avoids child→parent lock ordering deadlocks.
        if self.trait_.is_reference() {
            let parent = self.inner.lock().parent.upgrade();
            if let Some(parent) = parent {
                return parent.set_content_size_and_resize(size, zero_until_offset);
            }
        }
        // Read content_size and len under the lock, then drop it
        // before calling set_len (which also acquires the lock for
        // pager-backed VMO dirty tracking).
        let (content_size, len) = {
            let inner = self.inner.lock();
            (inner.content_size, self.trait_.len())
        };
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
        // Re-acquire the lock to update content_size.
        self.inner.lock().content_size = new_content_size;
        Ok(new_content_size)
    }

    /// Get the size of the content stored in the VMO in bytes.
    pub fn content_size(&self) -> usize {
        // REFERENCE children delegate to the parent so they always
        // reflect the parent's current content_size.
        if self.trait_.is_reference() {
            // Acquire parent Arc first, then drop the lock before
            // calling parent.content_size() to avoid child→parent
            // lock ordering issues.
            let parent = self.inner.lock().parent.upgrade();
            if let Some(parent) = parent {
                return parent.content_size();
            }
        }
        self.inner.lock().content_size
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

    /// Mark this VMO as discardable.
    pub fn set_discardable(&self) {
        self.discardable
            .store(true, core::sync::atomic::Ordering::Relaxed);
    }

    /// Check if this VMO is discardable.
    pub fn is_discardable(&self) -> bool {
        self.discardable.load(core::sync::atomic::Ordering::Relaxed)
    }

    /// Lock a discardable VMO, incrementing its lock count.
    ///
    /// Returns `NOT_SUPPORTED` if the VMO is not discardable, matching
    /// Fuchsia's `ZX_VMO_OP_LOCK` behavior.
    /// Lock a discardable VMO. If `try_lock` is true, returns
    /// `UNAVAILABLE` if the VMO has been discarded since it was last
    /// unlocked. Regular LOCK always succeeds (re-locks a discarded VMO).
    pub fn lock_discardable_ext(&self, try_lock: bool) -> ZxResult {
        if !self.is_discardable() {
            return Err(ZxError::NOT_SUPPORTED);
        }
        if try_lock && self.discarded.load(core::sync::atomic::Ordering::Relaxed) {
            return Err(ZxError::UNAVAILABLE);
        }
        // Clear discarded state on successful lock.
        self.discarded
            .store(false, core::sync::atomic::Ordering::Relaxed);
        let old = self
            .lock_count
            .fetch_add(1, core::sync::atomic::Ordering::Relaxed);
        // Track locked pages for KmemStatsExtended.
        if old == 0 {
            let pages = self.len() / PAGE_SIZE;
            VMO_DISCARDABLE_LOCKED_PAGES.add(pages);
        }
        Ok(())
    }

    /// Lock (non-try variant, for backward compatibility).
    pub fn lock_discardable(&self) -> ZxResult {
        self.lock_discardable_ext(false)
    }

    /// Unlock a discardable VMO. Returns BAD_STATE if already unlocked.
    /// When the lock count reaches 0, the VMO becomes eligible for
    /// reclamation. Pages are NOT immediately discarded — they remain
    /// committed until the kernel actively reclaims them. TRY_LOCK
    /// after unlock succeeds if pages haven't been reclaimed.
    pub fn unlock_discardable(&self) -> ZxResult {
        loop {
            let current = self.lock_count.load(core::sync::atomic::Ordering::Relaxed);
            if current == 0 {
                return Err(ZxError::BAD_STATE);
            }
            if self
                .lock_count
                .compare_exchange_weak(
                    current,
                    current - 1,
                    core::sync::atomic::Ordering::Relaxed,
                    core::sync::atomic::Ordering::Relaxed,
                )
                .is_ok()
            {
                if current == 1 {
                    // Lock count reached 0 — VMO is now unlocked and
                    // eligible for reclamation. Pages stay committed
                    // until actively reclaimed. Track for KmemStats.
                    let pages = self.len() / PAGE_SIZE;
                    VMO_DISCARDABLE_LOCKED_PAGES.sub(pages);
                    VMO_DISCARDABLE_UNLOCKED_PAGES.add(pages);
                }
                return Ok(());
            }
        }
    }

    /// content_size is properly initialized.
    pub fn set_content_size(&self, size: usize) -> ZxResult {
        // REFERENCE children delegate to the parent.
        // Acquire parent Arc first, drop the guard, then call —
        // avoids child→parent lock ordering deadlocks.
        if self.trait_.is_reference() {
            let parent = self.inner.lock().parent.upgrade();
            if let Some(parent) = parent {
                return parent.set_content_size(size);
            }
        }
        let old;
        {
            let mut inner = self.inner.lock();
            old = inner.content_size;
            inner.content_size = size;
        }
        // When content_size decreases, invalidate page table entries
        // beyond the new content_size in all mappings. This ensures
        // FAULT_BEYOND_STREAM_SIZE mappings correctly fault on accesses
        // to the now-beyond-stream-size region instead of reading stale
        // data from previously-mapped pages.
        if size < old {
            let start_page = size / PAGE_SIZE;
            let removed_pages = old.saturating_sub(size).div_ceil(PAGE_SIZE);
            if removed_pages > 0 {
                let inner = self.inner.lock();
                for map_weak in inner.mappings.iter() {
                    if let Some(map) = map_weak.upgrade() {
                        map.range_change(start_page, removed_pages, RangeChangeOp::Unmap);
                    }
                }
            }
        }
        Ok(())
    }

    /// Atomically read content_size and advance it by `count`, clamped
    /// to `max`. Returns the old content_size (the offset to write at).
    ///
    /// Used by stream append to reserve space without racing with
    /// concurrent appenders.
    pub fn fetch_add_content_size(&self, count: usize, max: usize) -> ZxResult<usize> {
        if self.trait_.is_reference() {
            let parent = self.inner.lock().parent.upgrade();
            if let Some(parent) = parent {
                return parent.fetch_add_content_size(count, max);
            }
        }
        let mut inner = self.inner.lock();
        let old = inner.content_size;
        let new = old.saturating_add(count).min(max);
        inner.content_size = new;
        Ok(old)
    }

    /// Set content_size and zero data beyond it.
    ///
    /// Used by `ZX_PROP_VMO_CONTENT_SIZE` set_property to maintain the
    /// invariant that data beyond content_size reads as zero. Zeros from
    /// the new content_size to the old content_size (or page boundary).
    pub fn set_content_size_with_zero(&self, size: usize) -> ZxResult {
        // REFERENCE children delegate to the parent.
        if self.trait_.is_reference() {
            let parent = self.inner.lock().parent.upgrade();
            if let Some(parent) = parent {
                return parent.set_content_size_with_zero(size);
            }
        }
        // content_size cannot exceed VMO size.
        if size > self.len() {
            return Err(ZxError::OUT_OF_RANGE);
        }
        let old = self.inner.lock().content_size;
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
        // set_content_size handles PTE invalidation on decrease.
        self.set_content_size(size)
    }

    /// Decommit pages in the given range.
    ///
    /// After decommitting, invalidates page table entries in all
    /// mappings (including on child/reference VMOs) so subsequent
    /// reads fault in zero pages.
    ///
    /// Returns `NOT_SUPPORTED` for pager-backed VMOs, matching Fuchsia's
    /// `ZX_VMO_OP_DECOMMIT` which rejects decommit on any VMO in a
    /// pager-backed tree (the pager owns page lifecycle).
    pub fn decommit(&self, offset: usize, len: usize) -> ZxResult {
        // Reject decommit on any VMO in a pager-backed tree
        // (including slices/references of pager-backed VMOs).
        // This matches Fuchsia's behavior: pager-backed VMOs don't
        // support decommit (pages are managed by the pager).
        if self.is_in_pager_tree() {
            return Err(ZxError::NOT_SUPPORTED);
        }
        // Validate range before decommitting.
        let end = offset.checked_add(len).ok_or(ZxError::OUT_OF_RANGE)?;
        if end > self.len() {
            return Err(ZxError::OUT_OF_RANGE);
        }
        // Invalidate PTEs BEFORE freeing frames. This prevents
        // another CPU from accessing a PTE that points to a
        // frame the allocator has already reused.
        self.invalidate_mappings(offset, len);
        self.trait_.decommit(offset, len)?;
        Ok(())
    }

    /// Invalidate PTEs in all mappings of this VMO and its children
    /// for the given range (offset and len in bytes).
    fn invalidate_mappings(&self, offset: usize, len: usize) {
        let inner = self.inner.lock();
        let start_page = offset / PAGE_SIZE;
        let page_count = len / PAGE_SIZE;
        for mapping_weak in &inner.mappings {
            if let Some(mapping) = mapping_weak.upgrade() {
                mapping.range_change(start_page, page_count, RangeChangeOp::Unmap);
            }
        }
        // Invalidate mappings on child VMOs. For slice/reference
        // children, translate the offset from parent to child
        // coordinates using the child's trait offset.
        let children: Vec<_> = inner.children.iter().filter_map(|c| c.upgrade()).collect();
        drop(inner);
        for child in children {
            // Translate parent offset to child coordinates.
            let child_offset = child.trait_.parent_offset();
            let child_len = child.trait_.len();
            // Compute intersection of [offset, offset+len) with
            // [child_offset, child_offset+child_len).
            let inter_start = offset.max(child_offset);
            let inter_end = (offset + len).min(child_offset + child_len);
            if inter_start < inter_end {
                child.invalidate_mappings(inter_start - child_offset, inter_end - inter_start);
            }
        }
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
                if self.discardable.load(core::sync::atomic::Ordering::Relaxed) {
                    f |= VmoInfoFlags::DISCARDABLE;
                }
                if inner.pager_port.is_some() {
                    f |= VmoInfoFlags::PAGER_BACKED;
                }
                f
            },
            cache_policy: self.trait_.cache_policy() as u32,
            share_count: inner.mapping_count as u64,
            // metadata_bytes: kernel overhead for VMO tracking.
            // 0 when no pages are committed; non-zero once pages exist.
            metadata_bytes: {
                let committed = self
                    .trait_
                    .committed_pages_in_range(0, self.trait_.len() / PAGE_SIZE)
                    as u64;
                if committed > 0 {
                    // Base overhead + per-page tracking
                    256 + committed * 64
                } else {
                    0
                }
            },
            ..Default::default()
        };
        self.trait_.complete_info(&mut ret);
        ret
    }

    /// Set the cache policy.
    pub fn set_cache_policy(&self, policy: CachePolicy) -> ZxResult {
        let mut inner = self.inner.lock();
        // Clean up dead weak refs before checking.
        inner.children.retain(|c| c.strong_count() > 0);
        if !inner.children.is_empty() {
            return Err(ZxError::BAD_STATE);
        }
        let before = inner.mappings.len();
        inner.mappings.retain(|m| m.strong_count() > 0);
        let removed = before - inner.mappings.len();
        inner.mapping_count = inner.mapping_count.saturating_sub(removed);
        if inner.mapping_count != 0 {
            return Err(ZxError::BAD_STATE);
        }
        drop(inner);
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

    /// Remove expired weak mapping references.
    pub fn cleanup_stale_mappings(&self) {
        let mut inner = self.inner.lock();
        let before = inner.mappings.len();
        inner.mappings.retain(|m| m.strong_count() > 0);
        let removed = before - inner.mappings.len();
        inner.mapping_count = inner.mapping_count.saturating_sub(removed);
    }

    /// Returns an estimate of the number of unique VmAspaces that this object
    /// is mapped into.
    pub fn share_count(&self) -> usize {
        let inner = self.inner.lock();
        inner.mapping_count
    }

    /// Returns true if the object size can be changed.
    /// For REFERENCE children, this delegates to the parent because
    /// a reference is a transparent alias — the parent's resizability
    /// determines whether the underlying VMO can be resized. Handle
    /// rights then control whether a particular handle is allowed to
    /// invoke the resize.
    pub fn is_resizable(&self) -> bool {
        if self.is_reference() {
            if let Some(parent) = self.inner.lock().parent.upgrade() {
                return parent.is_resizable();
            }
        }
        self.resizable
    }

    /// Whether this VMO has a parent (i.e., it's a child VMO).
    pub fn has_parent(&self) -> bool {
        self.inner.lock().parent.strong_count() > 0
    }

    /// Whether this VMO is a COW (copy-on-write) child.
    /// True for SNAPSHOT and SNAPSHOT_AT_LEAST_ON_WRITE children.
    pub fn is_cow_child(&self) -> bool {
        self.trait_.is_cow_child()
    }

    /// Whether the page at the given index is shared with the parent VMO
    /// (i.e., not yet copied for COW).
    pub fn page_is_shared(&self, page_idx: usize) -> bool {
        self.trait_.page_is_shared(page_idx)
    }

    /// Called when all handles to this VMO are closed.
    /// For child VMOs (slices, references, COW snapshots), invalidates
    /// all page table entries so accesses through existing mappings
    /// fault instead of returning stale data.
    ///
    /// Regular (non-child) VMOs are NOT affected — it's valid in
    /// Fuchsia to close a VMO handle and keep using the mapping.
    pub fn on_zero_handles_impl(&self) {
        // In Fuchsia, closing the last handle to a VMO does NOT
        // invalidate existing mappings. Mapped pages remain accessible
        // until the mapping is explicitly removed or the process exits.
        // The only effect is signaling ZX_VMO_ZERO_CHILDREN on the
        // parent (handled elsewhere via child list tracking).
    }

    pub fn is_dead(&self) -> bool {
        self.dead.load(core::sync::atomic::Ordering::Acquire)
    }

    /// Returns true if the VMO is backed by RAM (paged VMO).
    pub fn is_paged(&self) -> bool {
        self.trait_.is_paged()
    }

    pub fn is_contiguous(&self) -> bool {
        self.trait_.is_contiguous()
    }

    pub fn is_physical(&self) -> bool {
        self.trait_.is_physical()
    }

    /// Returns true if this VMO is a REFERENCE child.
    pub fn is_reference(&self) -> bool {
        self.trait_.is_reference()
    }

    /// Returns true if this VMO is a SLICE child (but not a REFERENCE).
    pub fn is_slice_vmo(&self) -> bool {
        self.is_slice
    }

    /// Offset into the parent VMO (for slices). Returns 0 for non-slices.
    pub fn parent_offset(&self) -> usize {
        self.trait_.parent_offset()
    }

    /// Check whether this VMO can create a unidirectional clone.
    ///
    /// Returns true if the cow_pages root is pager-backed and the
    /// cow_pages parent (if any) is not a hidden node.
    /// Mirrors Fuchsia's `VmCowPages::can_unidirectional_clone_locked`.
    pub fn can_unidirectional_clone(&self) -> bool {
        self.trait_.can_unidirectional_clone()
    }

    /// Get the parent VmObject (for REFERENCE or SLICE children).
    pub fn parent(&self) -> Option<Arc<VmObject>> {
        self.inner.lock().parent.upgrade()
    }

    /// Override the parent reference and move the child registration
    /// from the old parent to the new parent. Used when creating a
    /// child through a REFERENCE so the child's parent_koid points
    /// to the reference. Also ensures ZX_VMO_ZERO_CHILDREN is
    /// signaled correctly on the reference.
    pub fn set_parent_ref(self: &Arc<Self>, new_parent: &Arc<VmObject>) {
        // Remove from old parent's children list.
        let old_parent = self.inner.lock().parent.upgrade();
        if let Some(old_parent) = &old_parent {
            let mut old_inner = old_parent.inner.lock();
            old_inner
                .children
                .retain(|c| !c.upgrade().is_some_and(|c| Arc::ptr_eq(&c, self)));
            // Re-assert zero-children if needed.
            if old_inner.children.iter().all(|c| c.strong_count() == 0) {
                drop(old_inner);
                old_parent.base.signal_set(Signal::VMO_ZERO_CHILDREN);
            }
        }
        // Add to new parent's children list.
        new_parent.add_child(self);
        // Update parent weak ref.
        self.inner.lock().parent = Arc::downgrade(new_parent);
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
        // Send ZX_PAGER_VMO_COMPLETE to the pager port when the last
        // handle to a pager-backed VMO is closed.
        {
            let inner = self.inner.lock();
            let port = inner.pager_port.clone();
            if let Some(port) = port {
                port.push(PortPacketRepr {
                    key: inner.pager_key,
                    status: ZxError::OK,
                    data: PayloadRepr::PageRequest(PacketPageRequest {
                        command: ZX_PAGER_VMO_COMPLETE,
                        flags: 0,
                        _reserved0: 0,
                        offset: 0,
                        length: 0,
                        _reserved1: 0,
                    }),
                });
            }
        }
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
    pub num_mappings: u64,
    /// The number of unique address space we're mapped into.
    pub share_count: u64,
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
    pub metadata_bytes: u64,
    /// Running counter of committed-state change events.
    committed_change_events: u64,
    /// Content populated and tracked by this VMO (including shared pages).
    pub populated_bytes: u64,
    /// Physical memory allocated to only this VMO (not shared with clones).
    committed_private_bytes: u64,
    /// Content populated and tracked by only this VMO (not shared).
    populated_private_bytes: u64,
    /// `committed_bytes` scaled by sharing count (fractional bytes truncated).
    pub committed_scaled_bytes: u64,
    /// `populated_bytes` scaled by sharing count (fractional bytes truncated).
    pub populated_scaled_bytes: u64,
    /// Fractional remainder of `committed_scaled_bytes` (fixed-point, 63-bit precision).
    committed_fractional_scaled_bytes: u64,
    /// Fractional remainder of `populated_scaled_bytes` (fixed-point, 63-bit precision).
    /// Set to `u64::MAX` when fractional scaling is not supported.
    pub populated_fractional_scaled_bytes: u64,
}

impl VmoInfo {
    /// Get the koid of this VMO info entry.
    pub fn koid(&self) -> KoID {
        self.koid
    }

    /// Return the attributed populated bytes, matching Fuchsia's
    /// `VmoPopulatedBytes` test helper.
    ///
    /// When fractional scaling is supported (`populated_fractional_scaled_bytes
    /// != u64::MAX`), returns `populated_scaled_bytes` (fractional
    /// attribution). Otherwise falls back to `populated_bytes`.
    pub fn vmo_populated_bytes(&self) -> u64 {
        if self.populated_fractional_scaled_bytes == u64::MAX {
            self.populated_bytes
        } else {
            self.populated_scaled_bytes
        }
    }
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
