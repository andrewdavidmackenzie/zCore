use alloc::sync::Arc;
use core::sync::atomic::{AtomicBool, AtomicU64, AtomicU8, Ordering};
// use core::task::{RawWaker, RawWakerVTable};
use woke::Woke;

#[derive(Debug)]
pub struct AtomicU64SC(AtomicU64);
pub const WAKER_PAGE_SIZE: usize = 64;

impl AtomicU64SC {
    #[inline(always)]
    #[allow(unused)]
    pub fn new(val: u64) -> Self {
        AtomicU64SC(AtomicU64::new(val))
    }

    #[inline(always)]
    #[allow(unused)]
    pub fn fetch_or(&self, val: u64) {
        self.0.fetch_or(val, Ordering::SeqCst);
    }

    #[inline(always)]
    #[allow(unused)]
    pub fn fetch_and(&self, val: u64) {
        self.0.fetch_and(val, Ordering::SeqCst);
    }

    #[inline(always)]
    #[allow(unused)]
    pub fn fetch_add(&self, val: u64) -> u64 {
        self.0.fetch_add(val, Ordering::SeqCst)
    }

    #[inline(always)]
    #[allow(unused)]
    pub fn fetch_sub(&self, val: u64) -> u64 {
        self.0.fetch_sub(val, Ordering::SeqCst)
    }

    #[inline(always)]
    #[allow(unused)]
    pub fn load(&self) -> u64 {
        self.0.load(Ordering::SeqCst)
    }

    #[inline(always)]
    #[allow(unused)]
    pub fn swap(&self, val: u64) -> u64 {
        self.0.swap(val, Ordering::SeqCst)
    }

    #[inline(always)]
    #[allow(unused)]
    pub fn as_mut_ptr(&mut self) -> *mut u64 {
        self.0.as_ptr()
    }
}

/// A page is used by the scheduler to hold the current status of 64 different futures in the
/// scheduler. So we use 64bit integers where the ith bit represents the ith future. Pages are
/// arranged by the scheduler in a `pages` vector of pages which grows as needed allocating space
/// for 64 more futures at a time.
#[derive(Debug)]
#[repr(align(64))]
pub struct WakerPage {
    /// A 64 element bit vector representing the futures for this page which have been notified
    /// by a wake and are ready to be polled again. The ith bit represents the ith future in the
    /// corresponding memory slab.
    notified: AtomicU64SC,
    /// #862 diagnostics: total wake count per slot (saturating). Lets
    /// the stuck-task dumper distinguish a storm of wakes from a
    /// single lost wake.
    wake_counts: [AtomicU64SC; 64],
    // completed: AtomicU64SC,
    dropped: AtomicU64SC,
    borrowed: AtomicU64SC,
    /// CPU ID that owns this waker page. Used for cross-CPU IPI wakeup.
    cpu_id: AtomicU8,
}

impl WakerPage {
    pub fn new_inner() -> Self {
        WakerPage {
            notified: AtomicU64SC::new(0),
            wake_counts: core::array::from_fn(|_| AtomicU64SC::new(0)),
            // completed: AtomicU64SC::new(0),
            dropped: AtomicU64SC::new(0),
            borrowed: AtomicU64SC::new(0),
            cpu_id: AtomicU8::new(crate::arch::cpu_id()),
        }
    }

    pub fn new() -> Arc<Self> {
        Arc::new(WakerPage::new_inner())
    }

    /// Create a new WakerPage owned by a specific CPU.
    /// Used when a task is spawned on a remote CPU — the page must
    /// record the owning CPU, not the spawning CPU, so that IPI
    /// wakeups are sent to the correct target.
    pub fn new_for_cpu(cpu_id: u8) -> Arc<Self> {
        let mut page = WakerPage::new_inner();
        page.cpu_id = AtomicU8::new(cpu_id);
        Arc::new(page)
    }

    /// Get the owning CPU ID.
    pub fn owner_cpu(&self) -> u8 {
        self.cpu_id.load(Ordering::Relaxed)
    }

    pub fn initialize(&self, idx: usize) {
        debug_assert!(idx < 64);
        self.notified.fetch_or(1 << idx);
        // self.completed.fetch_and(!(1 << idx));
        self.dropped.fetch_and(!(1 << idx));
        self.borrowed.fetch_and(!(1 << idx));
    }

    pub fn mark_dropped(&self, idx: usize) {
        debug_assert!(idx < 64);
        self.dropped.fetch_or(1 << idx);
    }

    // pub fn mark_complete(&self, idx: usize) {
    //     debug_assert!(idx < 64);
    //     self.completed.fetch_or(1 << idx);
    // }

    /// #862 diagnostics: total wakes this slot has received.
    pub fn wake_count(&self, idx: usize) -> u64 {
        self.wake_counts[idx].load()
    }

    pub fn notify(&self, offset: usize) {
        debug_assert!(offset < 64);
        self.wake_counts[offset].fetch_add(1);
        self.notified.fetch_or(1 << offset);
    }

    pub fn mark_borrowed(&self, offset: usize, borrowed: bool) {
        debug_assert!(offset < 64);
        if borrowed {
            self.borrowed.fetch_or(1 << offset);
        } else {
            self.borrowed.fetch_and(!(1 << offset));
        }
    }

    // pub fn mark_completed(&self, offset: usize) {
    //     debug_assert!(offset < 64);
    //     self.completed.fetch_or(1 << offset);
    // }

    /// Return a bit vector representing the futures in this page which are ready to be
    /// polled again.
    ///
    /// Atomically takes all notified bits, then masks out futures that are currently
    /// borrowed (being polled) or dropped (completed). Any notifications that arrive
    /// while a future is borrowed would be lost by the `swap(0)`, so we restore
    /// those masked-out bits back into `notified` to ensure they are seen on the
    /// next call.
    pub fn take_notified(&self) -> u64 {
        let raw = self.notified.swap(0);
        let mask = !self.dropped.load() & !self.borrowed.load();
        let actionable = raw & mask;
        // Restore notifications we couldn't act on so they aren't lost.
        let deferred = raw & !mask;
        if deferred != 0 {
            self.notified.fetch_or(deferred);
        }
        actionable
    }

    /// Peek at notified bits without clearing them.
    pub fn peek_notified(&self) -> u64 {
        self.notified.load()
    }

    /// #862 diagnostics: peek dropped bits without consuming.
    pub fn peek_dropped(&self) -> u64 {
        self.dropped.load()
    }

    /// #862 diagnostics: peek borrowed (mid-poll) bits without
    /// consuming. A notification on a borrowed slot is deferred by
    /// `take_notified` until the poll finishes — it persists
    /// legitimately while a poll runs.
    pub fn peek_borrowed(&self) -> u64 {
        self.borrowed.load()
    }

    pub fn take_dropped(&self) -> u64 {
        self.dropped.swap(0)
    }

    pub fn clear(&self, idx: usize) {
        debug_assert!(idx < 64);
        let mask = !(1 << idx);
        self.notified.fetch_and(mask);
        // self.completed.fetch_and(mask);
        self.dropped.fetch_and(mask);
        self.borrowed.fetch_and(mask)
    }

    /// Create a `WakerRef` for a slot, with a fresh `WakerTarget`.
    /// All clones of this `WakerRef` share the same `WakerTarget`,
    /// which can be redirected during task migration.
    pub fn make_waker(self: &Arc<Self>, idx: usize, dropped: &Arc<AtomicBool>) -> WakerRef {
        WakerRef {
            target: WakerTarget::new(self.clone(), idx),
            dropped: dropped.clone(),
        }
    }

    /// Create a `WakerRef` that shares an existing `WakerTarget`.
    /// Used when re-creating a waker after task migration — the
    /// `WakerTarget` is shared with all previous waker clones so
    /// redirecting it updates everyone.
    pub fn make_waker_with_target(dropped: &Arc<AtomicBool>, target: Arc<WakerTarget>) -> WakerRef {
        WakerRef {
            target,
            dropped: dropped.clone(),
        }
    }
}

/// Indirection cell for waker targeting. Shared by all `WakerRef`
/// clones for a single task. When a task migrates between CPUs, the
/// `WakerTarget` is atomically updated to point at the new CPU's
/// waker page and slot index. Existing `Waker` clones captured by
/// the future's internals continue to work correctly because they
/// dereference the `WakerTarget` on every `wake_by_ref()` call.
pub struct WakerTarget {
    inner: spin::Mutex<WakerTargetInner>,
}

struct WakerTargetInner {
    page: Arc<WakerPage>,
    idx: usize,
}

impl WakerTarget {
    /// Create a new waker target pointing at the given page and slot.
    pub fn new(page: Arc<WakerPage>, idx: usize) -> Arc<Self> {
        Arc::new(Self {
            inner: spin::Mutex::new(WakerTargetInner { page, idx }),
        })
    }

    /// #862 diagnostics: (owner_cpu, slot_idx) this target points at.
    pub fn debug_target(&self) -> (u8, usize) {
        let inner = self.inner.lock();
        (inner.page.owner_cpu(), inner.idx)
    }

    /// Atomically redirect this waker target to a new page and slot.
    /// Called during task migration.
    pub fn redirect(&self, new_page: Arc<WakerPage>, new_idx: usize) {
        let mut inner = self.inner.lock();
        inner.page = new_page;
        inner.idx = new_idx;
    }

    /// Atomically redirect to a new page+slot AND clear the old slot,
    /// transferring any pending notification. This is called during
    /// task migration to ensure no wake is lost between redirect and
    /// old-slot clear.
    ///
    /// The sequence under the lock is:
    /// 1. Read old page's notified bit for our slot
    /// 2. Update inner to point at new page+slot
    /// 3. Clear old page slot (notified, dropped, borrowed)
    /// 4. If old slot was notified, set notified on new page
    ///
    /// Any concurrent `wake_by_ref()` will either:
    /// - Arrive before we lock: sets old page notified → we transfer it
    /// - Arrive after we lock: blocks until we're done, then uses new page
    pub fn redirect_and_clear_old(
        &self,
        new_page: Arc<WakerPage>,
        new_idx: usize,
        old_page: &WakerPage,
        old_idx: usize,
    ) {
        let mut inner = self.inner.lock();
        // Check if the old slot had a pending notification.
        let old_notified = old_page.peek_notified() & (1u64 << old_idx);
        // Switch to new target.
        inner.page = new_page;
        inner.idx = new_idx;
        // Clear old slot entirely.
        old_page.clear(old_idx);
        // Transfer notification to new slot if present, and IPI the
        // new CPU so it wakes from HLT/WFI to see the notification.
        if old_notified != 0 {
            inner.page.notify(inner.idx);
            crate::ipi_wakeup(inner.page.owner_cpu());
        }
    }

    /// Notify the current target page and send IPI to its CPU.
    fn notify(&self) {
        let inner = self.inner.lock();
        inner.page.notify(inner.idx);
        crate::ipi_wakeup(inner.page.owner_cpu());
    }

    /// Set or clear the borrowed bit on the current target page.
    fn mark_borrowed(&self, borrowed: bool) {
        let inner = self.inner.lock();
        inner.page.mark_borrowed(inner.idx, borrowed);
    }

    /// Mark the current target page slot as dropped.
    fn mark_dropped(&self) {
        let inner = self.inner.lock();
        inner.page.mark_dropped(inner.idx);
    }
}

pub type DroperRef = WakerRef;

pub struct WakerRef {
    target: Arc<WakerTarget>,
    dropped: Arc<AtomicBool>,
}

impl WakerRef {
    pub fn mark_borrowed(&self, borrowed: bool) {
        self.target.mark_borrowed(borrowed);
    }

    pub fn wake_by_ref(&self) {
        if !self.dropped.load(Ordering::SeqCst) {
            self.target.notify();
        }
    }

    pub fn drop_by_ref(&self) {
        if !self.dropped.swap(true, Ordering::SeqCst) {
            self.target.mark_dropped();
        }
    }

    /// Get the shared waker target. Used during task migration to
    /// redirect all existing waker clones to the new CPU's page.
    pub fn waker_target(&self) -> &Arc<WakerTarget> {
        &self.target
    }
}

impl Woke for WakerRef {
    fn wake_by_ref(waker: &Arc<Self>) {
        waker.wake_by_ref();
    }
}

impl Clone for WakerRef {
    fn clone(&self) -> Self {
        WakerRef {
            target: self.target.clone(),
            dropped: self.dropped.clone(),
        }
    }
}
