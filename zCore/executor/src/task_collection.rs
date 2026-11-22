use crate::waker_page::{DroperRef, WakerPage, WakerRef, WakerTarget, WAKER_PAGE_SIZE};
use alloc::sync::Arc;
use alloc::vec;
use alloc::vec::Vec;
use bit_iter::BitIter;
use core::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use slab::Slab;
use spin::{Mutex, MutexGuard};
use {
    alloc::boxed::Box,
    core::future::Future,
    core::pin::Pin,
    core::task::{Context, Poll},
};

use core::fmt::{Debug, Formatter, Result};

// #[allow(unused)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskState {
    // BLOCKED,
    RUNNABLE,
    RUNNING,
}

pub struct Task {
    id: usize,
    future: Mutex<Pin<Box<dyn Future<Output = ()> + Send>>>,
    inner: Mutex<TaskInner>,
    finish: Arc<AtomicBool>,
    /// Shared waker target for this task. All `WakerRef` clones
    /// (including those captured by the future's internals) share
    /// this target. During migration, the target is redirected to
    /// the new CPU's waker page slot, so existing wakers continue
    /// to reach the task on its new CPU.
    waker_target: Mutex<Option<Arc<WakerTarget>>>,
}

struct TaskInner {
    priority: usize,
    state: TaskState,
    intr_enable: bool,
}

impl core::fmt::Debug for Task {
    fn fmt(&self, f: &mut Formatter) -> Result {
        let inner = self.inner.lock();
        let mut f = f.debug_struct("X86PTE");
        f.field("priority", &inner.priority);
        f.field("state", &inner.state);
        f.field("intr_enable", &inner.intr_enable);
        f.finish()
    }
}

fn alloc_id() -> usize {
    static TASK_ID: AtomicUsize = AtomicUsize::new(1);
    TASK_ID.fetch_add(1, Ordering::SeqCst)
}

impl Task {
    pub fn new(future: impl Future<Output = ()> + Send + 'static, priority: usize) -> Self {
        Self {
            id: alloc_id(),
            future: Mutex::new(Box::pin(future)),
            inner: Mutex::new(TaskInner {
                priority,
                state: TaskState::RUNNABLE,
                intr_enable: true,
            }),
            finish: Arc::new(AtomicBool::new(false)),
            waker_target: Mutex::new(None),
        }
    }
    pub fn poll(&self, cx: &mut Context) -> Poll<()> {
        let mut f = self.future.lock();
        if self.inner.lock().intr_enable {
            crate::arch::intr_on();
        }
        let ret = f.as_mut().poll(cx);
        self.inner.lock().intr_enable = crate::arch::intr_get();
        crate::arch::intr_off();
        ret
    }

    pub fn id(&self) -> usize {
        self.id
    }

    /// Set the waker target for this task. Called when the task's
    /// waker is first created (in `take_task`).
    pub fn set_waker_target(&self, target: Arc<WakerTarget>) {
        *self.waker_target.lock() = Some(target);
    }

    /// Get the current waker target. Returns `None` if the task
    /// has never been polled (no waker created yet).
    pub fn waker_target(&self) -> Option<Arc<WakerTarget>> {
        self.waker_target.lock().clone()
    }
}

pub struct FutureCollection {
    pub slab: Slab<Arc<Task>>,
    // pub vec: VecDeque<Key>,
    pub pages: Vec<Arc<WakerPage>>,
    pub priority: usize,
    /// CPU ID that owns this collection. WakerPages created for this
    /// collection use this ID so IPI wakeups target the correct CPU.
    owner_cpu: u8,
}

impl FutureCollection {
    pub fn new(priority: usize, owner_cpu: u8) -> Self {
        Self {
            slab: Slab::new(),
            // vec: VecDeque::new(),
            pages: vec![],
            priority,
            owner_cpu,
        }
    }
    /// Our pages hold 64 contiguous future wakers, so we can do simple arithmetic to access the
    /// correct page as well as the index within page.
    /// Given the `key` representing a future, return a reference to that page, `Arc<WakerPage>`. And
    /// the index _within_ that page (usize).
    pub fn page(&self, key: Key) -> (&Arc<WakerPage>, usize) {
        let (_, page_idx, subpage_idx) = unpack_key(key);
        (&self.pages[page_idx], subpage_idx)
    }

    /// Insert a future into our scheduler returning an integer key representing this future. This
    /// key is used to index into the slab for accessing the future.
    pub fn insert<F: Future<Output = ()> + 'static + Send>(&mut self, future: F) -> (Key, usize) {
        let task = Arc::new(Task::new(future, self.priority));
        let task_id = task.id();
        let key = self.slab.insert(task);
        // Add a new page to hold this future's status if the current page is filled.
        while key >= self.pages.len() * WAKER_PAGE_SIZE {
            self.pages.push(WakerPage::new_for_cpu(self.owner_cpu));
        }
        let (page, subpage_idx) = self.page(key);
        page.initialize(subpage_idx);
        // self.vec.push_back(key);
        (key, task_id)
    }

    /// Insert an existing `Arc<Task>` (e.g., one stolen from another CPU).
    /// Returns the slab key (without priority bits). The slot is initialized
    /// as notified so it will be polled immediately.
    pub fn insert_task(&mut self, task: Arc<Task>) -> Key {
        let key = self.slab.insert(task);
        while key >= self.pages.len() * WAKER_PAGE_SIZE {
            self.pages.push(WakerPage::new_for_cpu(self.owner_cpu));
        }
        let (page, subpage_idx) = self.page(key);
        page.initialize(subpage_idx);
        key
    }

    /// Extract a task from this collection by slab key, clearing its
    /// waker page slot. Returns the `Arc<Task>` if found.
    pub fn extract(&mut self, key: Key) -> Option<Arc<Task>> {
        let slab_key = unmask_priority(key);
        if self.slab.contains(slab_key) {
            let (page, subpage_idx) = self.page(key);
            page.clear(subpage_idx);
            Some(self.slab.remove(slab_key))
        } else {
            None
        }
    }

    pub fn remove(&mut self, key: Key) -> bool {
        let (page, subpage_idx) = self.page(key);
        page.clear(subpage_idx);
        let slab_key = unmask_priority(key);
        if self.slab.contains(slab_key) {
            self.slab.remove(slab_key);
            true
        } else {
            false
        }
    }
}

pub struct TaskCollection {
    cpu_id: u8,
    future_collections: Vec<Mutex<FutureCollection>>,
    pub task_num: AtomicUsize,
    pub sched_state: Mutex<crate::sched::SchedState>,
    /// Key of a task that yielded voluntarily, pending re-queue.
    pending_yield: Mutex<Option<Key>>,
}

impl TaskCollection {
    pub fn new(cpu_id: u8) -> Arc<Self> {
        let mut task_collection = Arc::new(TaskCollection {
            cpu_id,
            future_collections: Vec::with_capacity(MAX_PRIORITY),
            task_num: AtomicUsize::new(0),
            sched_state: Mutex::new(crate::sched::new(cpu_id)),
            pending_yield: Mutex::new(None),
        });
        let tc = Arc::get_mut(&mut task_collection).expect("Arc just created, must be unique");
        for priority in 0..MAX_PRIORITY {
            tc.future_collections
                .push(Mutex::new(FutureCollection::new(priority, cpu_id)));
        }
        task_collection
    }

    /// Insert a Future with DEFAULT_PRIORITY.
    pub fn add_task<F: Future<Output = ()> + 'static + Send>(&self, future: F) -> (usize, usize) {
        self.priority_add_task(DEFAULT_PRIORITY, future)
    }

    /// Remove the task corresponding to the key.
    pub fn remove_task(&self, key: Key) {
        // Lock ordering: sched_state first, then future_collections.
        let mut sched = self.sched_state.lock();
        let mut inner = self.get_mut_inner(key >> PRIORITY_SHIFT);
        if inner.remove(unmask_priority(key)) {
            crate::sched::on_task_removed(&mut sched, key);
            self.task_num.fetch_sub(1, Ordering::Relaxed);
        }
    }

    pub fn priority_add_task<F: Future<Output = ()> + 'static + Send>(
        &self,
        priority: usize,
        future: F,
    ) -> (Key, usize) {
        debug_assert!(
            priority < MAX_PRIORITY,
            "priority {} >= MAX {}",
            priority,
            MAX_PRIORITY
        );
        // Lock ordering: sched_state first, then future_collections
        // (same as scan_notifications to avoid deadlock).
        // 5G-2: graph-checked sched_state acquisition — the entry
        // point for the sched→fc order.
        let mut locked = lock::lockgraph::Locked::new(self);
        let mut sched = locked.lock_and::<crate::lock_order::SchedStateLock>().0;
        let (key, task_id) = self.future_collections[priority].lock().insert(future);
        debug_assert!(key < TASK_NUM_PER_PRIORITY);
        let full_key = key | (priority << PRIORITY_SHIFT);
        crate::sched::on_task_added(&mut sched, full_key, priority);
        self.task_num.fetch_add(1, Ordering::Relaxed);
        (full_key, task_id)
    }

    fn get_mut_inner(&self, priority: usize) -> MutexGuard<'_, FutureCollection> {
        self.future_collections[priority].lock()
    }

    pub fn task_num(&self) -> usize {
        self.task_num.load(Ordering::Relaxed)
    }

    /// #862/#829 diagnostics: dump every task slot in this collection
    /// and the scheduler queues. Triggered when notified bits persist
    /// across many scans (a marked wake the scheduler never dispatched).
    /// Rate-limited to a few dumps per process.
    fn dump_stuck_state(&self) {
        use core::sync::atomic::Ordering;
        static DUMPS: core::sync::atomic::AtomicUsize = core::sync::atomic::AtomicUsize::new(0);
        if DUMPS.fetch_add(1, Ordering::Relaxed) >= 4 {
            return;
        }
        error!(
            "[stuck] cpu={} task_num={} sched: {}",
            self.cpu_id,
            self.task_num.load(Ordering::Relaxed),
            crate::sched::debug_summary(&self.sched_state.lock())
        );
        for priority in 0..MAX_PRIORITY {
            let inner = self.get_mut_inner(priority);
            for (slab_key, task) in inner.slab.iter() {
                let full_key = slab_key | (priority << PRIORITY_SHIFT);
                let page_idx = slab_key >> PAGE_INDEX_SHIFT;
                let subpage = slab_key & 63;
                let (n, d, owner, wc) = match inner.pages.get(page_idx) {
                    Some(p) => (
                        (p.peek_notified() >> subpage) & 1,
                        (p.peek_dropped() >> subpage) & 1,
                        p.owner_cpu(),
                        p.wake_count(subpage),
                    ),
                    None => (0, 0, 0xff, 0),
                };
                let ti = task.inner.lock();
                let wt = task.waker_target.lock();
                let target = wt.as_ref().map(|t| t.debug_target());
                error!(
                    "[stuck] key={:#x} id={} state={:?} prio={} notified={} dropped={} wake_count={} finish={} waker_target={:?}",
                    full_key,
                    task.id,
                    ti.state,
                    ti.priority,
                    n,
                    d,
                    wc,
                    task.finish.load(Ordering::Relaxed),
                    target
                );
            }
        }
    }

    /// Scan waker pages for notifications and dropped tasks.
    /// Feeds notified tasks into the scheduler and removes dropped ones.
    fn scan_notifications(&self) {
        use core::sync::atomic::{AtomicU64, Ordering as AtOrd};
        static SCAN_COUNT: AtomicU64 = AtomicU64::new(0);
        static FOUND_COUNT: AtomicU64 = AtomicU64::new(0);
        let scan_num = SCAN_COUNT.fetch_add(1, AtOrd::Relaxed);
        // Diagnostic cadence (#829 CI-stability hunt): every 10k scans
        // so hang-time state (remaining tasks + notified_bits) is
        // captured within a short hang. notified_bits != 0 while a
        // task never runs = the scheduler dropped a marked wake;
        // notified_bits == 0 with a parked task = the wake never
        // reached the waker page.
        if scan_num > 0 && scan_num % 10_000 == 0 {
            let found = FOUND_COUNT.load(AtOrd::Relaxed);
            let task_num = self.task_num.load(Ordering::Relaxed);
            if task_num > 0 {
                // Dump waker page state for this CPU
                let mut notified_bits = 0u64;
                let mut total_pages = 0usize;
                for priority in 0..MAX_PRIORITY {
                    let inner = self.get_mut_inner(priority);
                    for page_idx in 0..inner.pages.len() {
                        total_pages += 1;
                        let page = &inner.pages[page_idx];
                        let n = page.peek_notified();
                        notified_bits |= n;
                    }
                }
                warn!(
                    "scan: cpu={} scan#{} found={} tasks={} pages={} notified_bits={:#x}",
                    self.cpu_id, scan_num, found, task_num, total_pages, notified_bits
                );
                // #862: notified bits persisting across many scans = a
                // marked wake the scheduler never dispatched. Dump the
                // full collection + scheduler state (rate-limited).
                if notified_bits != 0 && scan_num >= 50_000 {
                    self.dump_stuck_state();
                }
            }
        }
        // 5G-2: graph-checked sched_state acquisition. The fc loop
        // below takes future_collections[priority] under sched — the
        // documented sched→fc order.
        let mut locked = lock::lockgraph::Locked::new(self);
        let mut sched = locked.lock_and::<crate::lock_order::SchedStateLock>().0;
        for priority in 0..MAX_PRIORITY {
            let mut inner = self.get_mut_inner(priority);
            for page_idx in 0..inner.pages.len() {
                let page = &inner.pages[page_idx];
                let notified = page.take_notified();
                let dropped = page.take_dropped();
                if notified != 0 {
                    FOUND_COUNT.fetch_add(notified.count_ones() as u64, AtOrd::Relaxed);
                    for subpage_idx in BitIter::from(notified) {
                        let key = pack_key(priority, page_idx, subpage_idx);
                        crate::sched::on_task_notified(&mut sched, key);
                    }
                }
                if dropped != 0 {
                    for subpage_idx in BitIter::from(dropped) {
                        let key = pack_key(priority, page_idx, subpage_idx);
                        if inner.remove(key) {
                            self.task_num.fetch_sub(1, Ordering::Relaxed);
                            crate::sched::on_task_removed(&mut sched, key);
                        }
                    }
                }
            }
        }
    }

    /// Change a task's scheduling priority.
    ///
    /// Updates the scheduler's priority mapping and moves the task
    /// between ready queues if it's currently enqueued.
    pub fn change_task_priority(&self, key: Key, new_priority: usize) {
        let mut sched = self.sched_state.lock();
        crate::sched::change_priority(&mut sched, key, new_priority);
    }

    /// Steal one runnable task from this collection directly into
    /// `dest` (another CPU's collection).
    ///
    /// The migration is atomic with respect to the waker system: the
    /// destination slot is reserved and initialized, and the task's
    /// waker target is redirected, all while the source's scheduler
    /// lock is held and the task is still in the source's slab. Only
    /// then is the source slot cleared and the slab entry removed.
    /// There is no window in which the task is reachable by neither
    /// CPU's scanner, so no wake-up can be lost in transit and the
    /// source's freed slot cannot be reused before it is retired.
    ///
    /// Only default-priority tasks are migrated. All other priorities
    /// (including Zircon threads at priority 16) are popped and
    /// re-queued so the source CPU keeps them.
    ///
    /// Lock order: source.sched → source.fc, then the destination
    /// locks via `try_lock` (never blocking). Blocking on a second
    /// collection's locks while holding the source's could deadlock
    /// against a mirror-image steal on another CPU; try_lock makes
    /// this function a leaf holder that always makes progress.
    pub fn steal_into(&self, dest: &TaskCollection) -> Option<usize> {
        // Lock ordering: sched_state first, then future_collections.
        // 5G-2: graph-checked source sched_state acquisition. The
        // destination's locks below use try_lock (the cross-CPU
        // same-type protocol — never blocking).
        let mut locked = lock::lockgraph::Locked::new(self);
        let mut sched = locked.lock_and::<crate::lock_order::SchedStateLock>().0;
        // Pick the lowest-priority runnable task from the scheduler.
        // Stealing the least-important task minimizes disruption to
        // the source CPU.
        let key = crate::sched::pick_lowest(&mut sched)?;
        // The key's priority bits are the SPAWN priority and can go
        // stale after change_task_priority(); the slab entry is
        // addressed by them, but migration eligibility must use the
        // scheduler's CURRENT priority.
        let storage_priority = key >> PRIORITY_SHIFT;
        if storage_priority >= MAX_PRIORITY {
            return None;
        }
        // Only default-priority tasks migrate. All other priorities
        // (including Zircon threads at 16, system tasks) are popped
        // and re-queued so the source CPU keeps them: migrating them
        // would disrupt time-sensitive threads (see #783).
        let current_priority = crate::sched::current_priority(&sched, key);
        if current_priority != DEFAULT_PRIORITY {
            crate::sched::on_task_notified(&mut sched, key);
            return None;
        }
        let mut inner = self.get_mut_inner(storage_priority);
        let slab_key = unmask_priority(key);
        if !inner.slab.contains(slab_key) {
            // Slab entry was already removed (race with take_task).
            return None;
        }
        let (_, page_idx, subpage_idx) = unpack_key(slab_key);
        if page_idx >= inner.pages.len() {
            return None;
        }
        let task: Arc<Task> = inner.slab.get(slab_key).unwrap().clone();

        // Reserve the destination slot while still holding the source
        // locks. The slot is initialized notified, so the destination
        // CPU's scanner will queue and poll the task.
        //
        // The destination locks are taken with try_lock: if another
        // CPU concurrently holds them (e.g. it is stealing from us),
        // we skip this attempt instead of blocking. Blocking here
        // while holding the source locks could deadlock against the
        // other CPU doing the mirror-image steal.
        let mut dest_sched = match dest.sched_state.try_lock() {
            Some(guard) => guard,
            None => {
                // Put the popped key back so the source keeps the task.
                crate::sched::on_task_notified(&mut sched, key);
                return None;
            }
        };
        let mut dest_inner = match dest.future_collections[current_priority].try_lock() {
            Some(guard) => guard,
            None => {
                drop(dest_sched);
                crate::sched::on_task_notified(&mut sched, key);
                return None;
            }
        };
        let new_slab_key = dest_inner.insert_task(task.clone());
        // The new key encodes the CURRENT priority so the destination's
        // take_task finds the slab entry in the matching collection.
        let new_full_key = new_slab_key | (current_priority << PRIORITY_SHIFT);
        let (_, new_page_idx, new_subpage_idx) = unpack_key(new_slab_key);

        // Redirect the task's waker target to the destination slot and
        // retire the source slot, atomically under the target's lock:
        // a concurrent wake either lands on the source slot before we
        // take the lock (its pending bit is transferred by the
        // redirect) or on the destination slot after we release it.
        if let Some(target) = task.waker_target() {
            target.redirect_and_clear_old(
                dest_inner.pages[new_page_idx].clone(),
                new_subpage_idx,
                &inner.pages[page_idx],
                subpage_idx,
            );
        } else {
            // Never polled — no wakers exist, so nothing can be lost.
            inner.pages[page_idx].clear(subpage_idx);
        }

        // Detach from the source. The slab entry is removed only after
        // the waker target points at the destination, and the source
        // slot was already cleared under the target lock above.
        inner.slab.remove(slab_key);
        drop(dest_inner);
        crate::sched::on_task_added(&mut dest_sched, new_full_key, current_priority);
        drop(dest_sched);
        drop(inner);
        crate::sched::on_task_removed(&mut sched, key);
        drop(sched);
        self.task_num.fetch_sub(1, Ordering::Relaxed);
        dest.task_num.fetch_add(1, Ordering::Relaxed);
        Some(task.id())
    }

    /// Store the key of a task that yielded voluntarily.
    pub fn set_pending_yield(&self, key: Key) {
        *self.pending_yield.lock() = Some(key);
    }

    /// Force-set the notified bit for `key`'s waker slot and IPI this
    /// collection's CPU, after verifying the slot still holds
    /// `task_id`. Returns `false` if the slot holds a different task
    /// (stale key, e.g. after a work-stealing migration recycled it)
    /// or the task is gone.
    ///
    /// Used at process exit to re-fire kill wake-ups that may have
    /// been lost, so shutdown never depends on perfect wake delivery.
    /// A spurious re-notify is harmless: the task is simply polled
    /// once more and re-parks if its wait is not satisfied.
    pub fn force_notify_verified(&self, key: Key, task_id: usize) -> bool {
        let priority = key >> PRIORITY_SHIFT;
        if priority >= MAX_PRIORITY {
            return false;
        }
        // Disable interrupts while holding the collection lock so a
        // timer preemption cannot context-switch away with the lock
        // held (the lock would stay held until the preempted context
        // resumes).
        let enable = crate::arch::intr_get();
        if enable {
            crate::arch::intr_off();
        }
        let page = {
            let inner = self.future_collections[priority].lock();
            let slab_key = unmask_priority(key);
            match inner.slab.get(slab_key) {
                Some(task) if task.id() == task_id => {
                    let (_, page_idx, subpage_idx) = unpack_key(slab_key);
                    if page_idx >= inner.pages.len() {
                        None
                    } else {
                        Some((inner.pages[page_idx].clone(), subpage_idx))
                    }
                }
                _ => None,
            }
        };
        if enable {
            crate::arch::intr_on();
        }
        if let Some((page, subpage_idx)) = page {
            page.notify(subpage_idx);
            crate::ipi_wakeup(self.cpu_id);
            true
        } else {
            false
        }
    }

    /// Find a task by id anywhere in this collection and force-notify
    /// its waker slot. Returns `true` if the task was found.
    ///
    /// The fallback path of the process-exit sweep: work stealing can
    /// migrate a task to another CPU, leaving the location saved by
    /// `Thread::start` stale; this resolves the task's current home.
    pub fn force_notify_by_id(&self, task_id: usize) -> bool {
        for priority in 0..MAX_PRIORITY {
            // Disable interrupts while holding the collection lock so a
            // timer preemption cannot context-switch away with the lock
            // held (the lock would stay held until the preempted context
            // resumes).
            let enable = crate::arch::intr_get();
            if enable {
                crate::arch::intr_off();
            }
            let page = {
                let inner = self.future_collections[priority].lock();
                let mut found = None;
                for (slab_key, task) in inner.slab.iter() {
                    if task.id() == task_id {
                        let (_, page_idx, subpage_idx) = unpack_key(slab_key);
                        if page_idx < inner.pages.len() {
                            found = Some((inner.pages[page_idx].clone(), subpage_idx));
                        }
                        break;
                    }
                }
                found
            };
            if enable {
                crate::arch::intr_on();
            }
            if let Some((page, subpage_idx)) = page {
                page.notify(subpage_idx);
                crate::ipi_wakeup(self.cpu_id);
                return true;
            }
        }
        false
    }

    /// Select the next task to run using the scheduler.
    pub fn take_task(&self) -> Option<(Key, Arc<Task>, WakerRef, DroperRef)> {
        self.scan_notifications();
        // Re-queue any task that yielded AFTER scanning notifications.
        // This ensures newly-notified tasks (e.g., child threads) are
        // in the queue BEFORE the yielding task, giving them priority.
        {
            let mut sched = self.sched_state.lock();
            if let Some(yield_key) = self.pending_yield.lock().take() {
                crate::sched::on_yield(&mut sched, yield_key);
            }
        }
        let key = crate::sched::pick_next(&mut self.sched_state.lock())?;
        let (priority, page_idx, subpage_idx) = unpack_key(key);
        let inner = self.get_mut_inner(priority);
        let task = inner.slab.get(unmask_priority(key))?.clone();
        // If the task already has a WakerTarget (e.g., from a previous
        // poll or migration), reuse it so all existing waker clones
        // share the same target. Otherwise create a fresh one.
        let waker = if let Some(existing_target) = task.waker_target() {
            // Redirect the existing target to this page+idx (may have
            // changed due to migration or slab reuse).
            existing_target.redirect(inner.pages[page_idx].clone(), subpage_idx);
            WakerPage::make_waker_with_target(&task.finish, existing_target)
        } else {
            let w = inner.pages[page_idx].make_waker(subpage_idx, &task.finish);
            task.set_waker_target(w.waker_target().clone());
            w
        };
        let droper = waker.clone();
        Some((key, task, waker, droper))
    }
}

pub use key::*;

pub mod key {
    pub type Key = usize;
    pub const PRIORITY_SHIFT: usize = 58;
    pub const TASK_NUM_PER_PRIORITY: usize = 1 << PRIORITY_SHIFT;
    pub const MAX_PRIORITY: usize = 1 << 5;
    pub const DEFAULT_PRIORITY: usize = 4;

    pub const PAGE_INDEX_SHIFT: usize = 6;

    pub fn unpack_key(key: Key) -> (usize, usize, usize) {
        let subpage_idx = key & 0x3F;
        let priority = key >> PRIORITY_SHIFT;
        // Mask out the priority bits before extracting page_idx.
        // The old formula `(key << 5) >> 11` overflowed for priorities >= 12,
        // corrupting page_idx with leftover priority bits.
        let page_idx = (key & ((1 << PRIORITY_SHIFT) - 1)) >> PAGE_INDEX_SHIFT;
        (priority, page_idx, subpage_idx)
    }

    pub fn pack_key(priority: usize, page_idx: usize, subpage_idx: usize) -> Key {
        (priority << PRIORITY_SHIFT) | (page_idx << PAGE_INDEX_SHIFT) | subpage_idx
    }

    pub fn unmask_priority(key: Key) -> usize {
        key & !(0x1F << PRIORITY_SHIFT)
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn pack_unpack_roundtrip() {
            for priority in 0..MAX_PRIORITY {
                for page_idx in [0, 1, 10, 100] {
                    for subpage_idx in [0, 1, 63] {
                        let key = pack_key(priority, page_idx, subpage_idx);
                        let (p, pg, sp) = unpack_key(key);
                        assert_eq!(
                            (p, pg, sp),
                            (priority, page_idx, subpage_idx),
                            "roundtrip failed for priority={priority} page={page_idx} sub={subpage_idx} key={key:#x}"
                        );
                    }
                }
            }
        }

        #[test]
        fn unmask_priority_strips_top_bits() {
            for priority in 0..MAX_PRIORITY {
                let key = pack_key(priority, 42, 7);
                let slab_key = unmask_priority(key);
                assert_eq!(slab_key, (42 << PAGE_INDEX_SHIFT) | 7);
            }
        }
    }
}
