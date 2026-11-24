use crate::{object::*, task::Thread};
use alloc::collections::VecDeque;
use alloc::sync::Arc;
use core::future::Future;
use core::pin::Pin;
use core::sync::atomic::*;
use core::task::{Context, Poll, Waker};
use lock::IrqLock;

/// A primitive for creating userspace synchronization tools.
///
/// ## SYNOPSIS
/// A **futex** is a Fast Userspace muTEX. It is a low level
/// synchronization primitive which is a building block for higher level
/// APIs such as `pthread_mutex_t` and `pthread_cond_t`.
/// Futexes are designed to not enter the kernel or allocate kernel
/// resources in the uncontested case.
pub struct Futex {
    base: KObjectBase,
    /// User-space address of the futex word.
    user_addr: usize,
    pub(super) inner: IrqLock<FutexInner>,
}

impl_kobject!(Futex);

#[derive(Default)]
pub struct FutexInner {
    waiter_queue: VecDeque<Arc<Waiter>>,
    /// NOTE: use `set_owner`
    owner: Option<Arc<Thread>>,
}

impl Futex {
    /// Create a new Futex keyed by a user-space address.
    ///
    /// `user_addr` is the virtual address of the user-space `i32` that
    /// threads will wait on. The kernel atomically loads this value
    /// (via `load_user_value`) to check for spurious wakeups.
    pub fn new(user_addr: usize) -> Arc<Self> {
        Arc::new(Futex {
            base: KObjectBase::default(),
            user_addr,
            inner: IrqLock::new(FutexInner::default()),
        })
    }

    /// Atomically load the i32 at the user-space futex address.
    ///
    /// Uses `stac`/`clac` on x86_64 to temporarily allow kernel
    /// access to user pages when SMAP is enabled.
    #[allow(unsafe_code)]
    fn load_user_value(&self) -> i32 {
        let ptr = self.user_addr as *const AtomicI32;
        unsafe {
            hal_impl::user::smap_allow();
            let val = (*ptr).load(Ordering::SeqCst);
            hal_impl::user::smap_deny();
            val
        }
    }

    /// Wait on a futex.
    ///
    /// This atomically verifies that `value_ptr` still contains the value `current_value`
    /// and sleeps until the futex is made available by a call to [`wake`].
    ///
    /// See [`wait_with_owner`] for advanced usage and more details.
    ///
    /// [`wait_with_owner`]: Futex::wait_with_owner
    /// [`wake`]: Futex::wake
    pub fn wait(self: &Arc<Self>, current_value: i32) -> impl Future<Output = ZxResult> {
        self.wait_with_owner(current_value, None, None)
    }

    /// Wake some number of threads waiting on a futex.
    ///
    /// It wakes at most `wake_count` of the waiters that are waiting on this futex.
    /// Return the number of waiters that were woken up.
    ///
    /// # Ownership
    ///
    /// The owner of the futex is set to nothing, regardless of the wake count.
    pub fn wake(&self, wake_count: usize) -> usize {
        // 5F-2: graph-checked futex acquisition. Collect waiters under
        // the futex lock, then wake them after releasing it — the
        // collect-then-act pattern that avoids the #102 ABBA.
        // (FutexInnerLock → WaiterInnerLock is the declared edge; the
        // graph would permit nested acquisition in that direction,
        // but the code deliberately releases before waking to keep
        // the futex lock hold time minimal.)
        let waiters: alloc::vec::Vec<_> = {
            let mut locked = lock::lockgraph::Locked::new(self);
            let mut inner = locked.lock_and::<super::lock_order::FutexInnerLock>().0;
            inner.set_owner(None);
            // INV-FUTEX-3: the owner is cleared regardless of the wake
            // count (asserted after the collection loop too, so a
            // regression in the loop can't reintroduce an owner).
            inv!(
                inner.owner.is_none(),
                "INV-FUTEX-3: owner not cleared by wake"
            );
            #[cfg(feature = "invariant-checks")]
            let queued = inner.waiter_queue.len();
            #[cfg(feature = "invariant-checks")]
            let departing_queued = inner
                .waiter_queue
                .iter()
                .filter(|w| w.inner.lock().departing)
                .count();
            let mut collected = alloc::vec::Vec::new();
            let mut remaining = wake_count;
            while remaining > 0 {
                if let Some(waiter) = inner.waiter_queue.pop_front() {
                    // #862: skip departing waiters (their FutexFuture
                    // is dropping — a BAD_STATE retry or timeout). A
                    // departing waiter must not consume a wake-count
                    // slot: for wake(1) callers (mutex unlock) that
                    // would permanently lose the wake for the real
                    // waiter remaining in the queue. Linux's futex
                    // wake loop skips FUTEX_STATE_EXITING waiters the
                    // same way.
                    if waiter.inner.lock().departing {
                        continue;
                    }
                    collected.push(waiter);
                    remaining -= 1;
                } else {
                    break;
                }
            }
            // INV-FUTEX-1: at most wake_count, at most the queued
            // waiters (zx_futex_wake).
            inv!(
                collected.len() <= wake_count && collected.len() <= queued,
                "INV-FUTEX-1: wake({}) returned {} with {} queued",
                wake_count,
                collected.len(),
                queued
            );
            // INV-FUTEX-2: departing waiters never consume a wake
            // slot — exactly min(wake_count, non-departing queued)
            // (#862/#875).
            inv!(
                collected.len() == wake_count.min(queued - departing_queued),
                "INV-FUTEX-2: wake({}) returned {} with {} queued ({} departing)",
                wake_count,
                collected.len(),
                queued,
                departing_queued
            );
            inv!(inner.owner.is_none(), "INV-FUTEX-3: owner set during wake");
            collected
        };
        let count = waiters.len();
        // #862 diagnostics: waking fewer waiters than requested means
        // either nobody was waiting (normal when the wake races the
        // waiters' registration — they will retry with BAD_STATE) or a
        // waiter departed (Drop) consuming the count. With ALL-wakes
        // (barrier), a short queue during contention highlights the
        // registration race window.
        if count < wake_count && count > 0 {
            trace!(
                "[futex-wake] addr={:#x} requested={} woken={} (short queue)",
                self.user_addr,
                wake_count,
                count
            );
        }
        for waiter in &waiters {
            waiter.wake();
        }
        count
    }

    // ------ Advanced APIs on Zircon ------

    /// The user-space address this futex is keyed by.
    pub fn user_addr(&self) -> usize {
        self.user_addr
    }

    /// Get the owner of the futex.
    pub fn owner(&self) -> Option<Arc<Thread>> {
        self.inner.lock().owner.clone()
    }

    /// Try to get the owner without blocking. Returns `None` if the
    /// futex inner lock is already held (avoids deadlock in the
    /// transitive priority inheritance path).
    pub fn try_owner(&self) -> Option<Arc<Thread>> {
        self.inner.try_lock().and_then(|g| g.owner.clone())
    }

    /// Wait on a futex.
    ///
    /// This atomically verifies that `value_ptr` still contains the value `current_value`
    /// and sleeps until the futex is made available by a call to [`wake`].
    ///
    /// # SPURIOUS WAKEUPS
    ///
    /// This implementation currently does not generate spurious wakeups.
    ///
    /// # Ownership
    ///
    /// A successful call results in the owner of the futex being set to the
    /// thread referenced by the `new_owner`, or to nothing if it is `None`.
    ///
    /// # Errors
    ///
    /// - `INVALID_ARGS`: One of the following is true
    ///   - `new_owner` is currently a member of the waiters for this.
    ///   - `new_owner` has not been started yet.
    /// - `BAD_STATE`: `current_value` does not match the value at `value_ptr`.
    /// - `TIMED_OUT`: The thread was not woken before deadline passed.
    ///
    /// [`wake`]: Futex::wake
    pub fn wait_with_owner(
        self: &Arc<Self>,
        current_value: i32,
        thread: Option<Arc<Thread>>,
        new_owner: Option<Arc<Thread>>,
    ) -> impl Future<Output = ZxResult> {
        #[must_use = "wait does nothing unless polled/`await`-ed"]
        struct FutexFuture {
            /// #862 diagnostics: BAD_STATE retries on this wait (the
            /// contention-retry loop).
            bad_state_retries: usize,
            /// Whether poll() ever pushed this waiter into the futex
            /// queue. Distinguishes "popped by a wake during drop"
            /// (queued=true, not in queue → re-transfer the wake) from
            /// "never queued" (validation failed before the push →
            /// nothing to re-transfer).
            queued: bool,
            waiter: Arc<Waiter>,
            current_value: i32,
            new_owner: Option<Arc<Thread>>,
        }
        impl Future for FutexFuture {
            type Output = ZxResult;

            fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
                // 5F-2: graph-checked waiter acquisition. The poll
                // path deliberately does NOT hold WaiterInner across
                // FutexInner — the collect-then-act pattern. Each
                // acquisition below enters from Unlocked (or from a
                // context whose previous guard has been dropped),
                // keeping the #102 ABBA structurally impossible.
                let mut locked = lock::lockgraph::Locked::new(self.waiter.as_ref());
                let mut inner = locked.lock_and::<super::lock_order::WaiterInnerLock>().0;
                // check wakeup
                if inner.woken {
                    return Poll::Ready(Ok(()));
                }
                // first time?
                if inner.waker.is_none() {
                    // Clone the futex reference and waker before
                    // releasing the inner lock. This avoids the borrow
                    // checker issue where inner.futex (immutable) and
                    // inner.waker (mutable) can't coexist.
                    let futex_ref = inner.futex.clone();
                    let waker = cx.waker().clone();
                    // Store the waker FIRST so a wake that pops us from
                    // the queue after registration can call it.
                    inner.waker.replace(waker);
                    drop(inner);

                    // Now lock the futex and do the value check +
                    // waiter registration. Graph-checked: the futex
                    // lock is acquired from a fresh context (the
                    // waiter guard was dropped above).
                    let mut futex_locked = lock::lockgraph::Locked::new(futex_ref.as_ref());
                    let mut futex = futex_locked
                        .lock_and::<super::lock_order::FutexInnerLock>()
                        .0;
                    let value = futex_ref.load_user_value();
                    if value != self.current_value {
                        // Value changed — cancel the wait. Remove the
                        // waker we just stored.
                        // #862 diagnostics: a thread retrying the same
                        // futex wait with a mismatched value hundreds of
                        // times = the barrier/mutex retry livelock.
                        self.bad_state_retries += 1;
                        if self.bad_state_retries.is_multiple_of(100) {
                            warn!(
                                "[futex-retry] thread={:?} addr={:#x} expected={} actual={} retries={}",
                                self.waiter.thread.as_ref().map(|t| t.id()),
                                futex_ref.user_addr,
                                self.current_value,
                                value,
                                self.bad_state_retries
                            );
                        }
                        let mut locked2 = lock::lockgraph::Locked::new(self.waiter.as_ref());
                        let mut inner2 = locked2.lock_and::<super::lock_order::WaiterInnerLock>().0;
                        inner2.waker.take();
                        return Poll::Ready(Err(ZxError::BAD_STATE));
                    }
                    if !futex.is_valid_new_owner(&self.new_owner, futex_ref.user_addr) {
                        let mut locked2 = lock::lockgraph::Locked::new(self.waiter.as_ref());
                        let mut inner2 = locked2.lock_and::<super::lock_order::WaiterInnerLock>().0;
                        inner2.waker.take();
                        return Poll::Ready(Err(ZxError::INVALID_ARGS));
                    }
                    futex.set_owner(self.new_owner.clone());
                    futex.waiter_queue.push_back(self.waiter.clone());
                    self.queued = true;
                    if let Some(ref owner) = futex.owner {
                        if let Some(ref waiter_thread) = self.waiter.thread {
                            let waiter_pri = waiter_thread.effective_priority();
                            if waiter_pri > owner.effective_priority() {
                                owner.boost_priority(waiter_pri);
                            }
                        }
                    }
                    if let Some(ref thread) = self.waiter.thread {
                        thread.set_blocking_futex(futex_ref.user_addr);
                    }
                    drop(futex);

                    // Re-check `woken` after registering in the queue.
                    // A wake that ran between storing the waker (above)
                    // and pushing into the queue found an empty queue
                    // and was lost — `woken` is still false because
                    // `Waiter::wake()` was never called on us. But a
                    // wake that ran AFTER the push but BEFORE we
                    // re-acquire the waiter lock here would have popped
                    // us and set `woken=true` (and called our waker,
                    // which schedules a re-poll). The re-check catches
                    // that case and returns Ready immediately instead
                    // of parking on a notification that already
                    // happened.
                    let mut locked2 = lock::lockgraph::Locked::new(self.waiter.as_ref());
                    let inner2 = locked2.lock_and::<super::lock_order::WaiterInnerLock>().0;
                    if inner2.woken {
                        return Poll::Ready(Ok(()));
                    }
                    drop(inner2);
                }
                Poll::Pending
            }
        }
        // The FutexFuture will be dropped when the thread is no longer waiting
        // if we wake without be woken, remove myself from the waiter_queue
        impl Drop for FutexFuture {
            fn drop(&mut self) {
                // 5F-2: restructured from the original nested
                // acquisition (WaiterInner held across FutexInner —
                // the #102 ABBA direction) to collect-then-act.
                // The original nesting prevented a race where wake()
                // pops the waiter between dropping WaiterInner and
                // taking FutexInner. The restructure handles this by
                // checking queue membership under FutexInner: if
                // wake() already popped us, we won't find ourselves
                // in the queue and the removal is a no-op.
                let futex_arc = {
                    let mut locked = lock::lockgraph::Locked::new(self.waiter.as_ref());
                    let mut inner = locked.lock_and::<super::lock_order::WaiterInnerLock>().0;
                    if inner.woken {
                        // Already woken — wake() popped us from the
                        // queue. Just clear tracking.
                        if let Some(ref thread) = self.waiter.thread {
                            thread.set_blocking_futex(0);
                        }
                        return;
                    }
                    // Mark departing so a concurrent Futex::wake that
                    // pops us skips us and wakes the next waiter
                    // instead of consuming the wake count (#862).
                    inner.departing = true;
                    inner.futex.clone()
                }; // WaiterInner released

                // Take FutexInner from a fresh context (legal — the
                // waiter guard was dropped above).
                let mut futex_locked = lock::lockgraph::Locked::new(futex_arc.as_ref());
                let mut futex = futex_locked
                    .lock_and::<super::lock_order::FutexInnerLock>()
                    .0;
                // Check queue membership: if a wake raced between
                // releasing WaiterInner and acquiring FutexInner and
                // already popped us, this is a no-op.
                let in_queue = futex
                    .waiter_queue
                    .iter()
                    .position(|x| alloc::sync::Arc::ptr_eq(x, &self.waiter));
                if let Some(pos) = in_queue {
                    futex.waiter_queue.swap_remove_back(pos);
                    // Priority inheritance: recompute owner's boost
                    // after removing a waiter.
                    if let Some(ref owner) = futex.owner {
                        let max_pri = futex.max_waiter_priority();
                        if max_pri > 0 {
                            owner.boost_priority(max_pri);
                        } else {
                            owner.clear_pi_boost();
                        }
                    }
                }
                // #862: the waiter was neither woken nor in the
                // queue — a wake popped it in the window before our
                // departing mark was visible, consuming a wake-count
                // slot for a waiter that is departing. Re-transfer
                // the wake to the next queued waiter: the departing
                // thread returns TIMED_OUT/BAD_STATE and its
                // userspace retry re-checks the value anyway (Zircon
                // delivers this wake to the thread as a spurious
                // ZX_OK; we can't — the future is already dropped —
                // so we pass it on instead).
                let next_waiter = if in_queue.is_none() && self.queued {
                    futex.waiter_queue.pop_front()
                } else {
                    None
                };
                drop(futex);
                if let Some(waiter) = next_waiter {
                    waiter.wake();
                    return;
                }
                // Clear blocking futex tracking.
                if let Some(ref thread) = self.waiter.thread {
                    thread.set_blocking_futex(0);
                }
            }
        }
        FutexFuture {
            bad_state_retries: 0,
            queued: false,
            waiter: Arc::new(Waiter {
                thread,
                inner: IrqLock::new(WaiterInner {
                    waker: None,
                    woken: false,
                    departing: false,
                    futex: self.clone(),
                }),
            }),
            current_value,
            new_owner,
        }
    }

    /// Wake exactly one thread from the futex wait queue.
    ///
    /// If there is at least one thread to wake, the owner of the futex will
    /// be set to the thread which was woken. Otherwise, the futex will have
    /// no owner.
    ///
    /// # Ownership
    ///
    /// If there is at least one thread to wake, the owner of the futex will be
    /// set to the thread which was woken. Otherwise, the futex will have no owner.
    pub fn wake_single_owner(&self) {
        // 5F-2: graph-checked. Pop waiter and set owner under the
        // lock, then wake after releasing (collect-then-act — the
        // same pattern as wake()).
        let (waiter_to_wake, new_owner) = {
            let mut locked = lock::lockgraph::Locked::new(self);
            let mut inner = locked.lock_and::<super::lock_order::FutexInnerLock>().0;
            // #862: skip departing waiters — same rationale as
            // Futex::wake (a departing waiter must not consume the
            // single wake slot).
            let popped = loop {
                match inner.waiter_queue.pop_front() {
                    Some(w) if w.inner.lock().departing => continue,
                    other => break other,
                }
            };
            let owner = popped.as_ref().and_then(|w| w.thread.clone());
            inner.set_owner(owner.clone());
            (popped, owner)
        };
        if let Some(waiter) = waiter_to_wake {
            waiter.wake();
        }
        let _ = new_owner; // used only for set_owner above
    }

    /// Requeuing is a generalization of waking.
    ///
    /// First, verifies that the value in `current_value` matches the value of the futex,
    /// and if not reports `ZxError::BAD_STATE`. After waking `wake_count` threads,
    /// `requeue_count` threads are moved from the original futex's wait queue to the
    /// wait queue corresponding to another `requeue_futex`.
    ///
    /// This requeueing behavior may be used to avoid thundering herds on wake.
    ///
    /// # Ownership
    ///
    /// The owner of this futex is set to nothing, regardless of the wake count.
    /// The owner of the `requeue_futex` is set to the thread `new_requeue_owner`.
    /// Wakes up to `wake_count` waiters and moves up to `requeue_count`
    /// remaining waiters to `requeue_futex`. If `check_value` is true,
    /// first verifies that `*uaddr == current_value` (for CMP_REQUEUE).
    /// Returns the total number of waiters woken plus requeued on success.
    ///
    /// The two futex locks are acquired in a consistent order (by pointer
    /// identity) to prevent deadlocks between concurrent requeue calls
    /// with swapped source/target addresses. This is a same-type
    /// protocol — not graph-expressible (which futex is "first" depends
    /// on runtime addresses), like the socket endpoint pair and the COW
    /// tree. The FutexInner ↔ WaiterInner ordering is enforced by the
    /// lockgraph (5F-1); the #102 concern is resolved by the
    /// collect-then-act restructuring in wake/poll/Drop (5F-2).
    pub fn requeue(
        &self,
        current_value: i32,
        wake_count: usize,
        requeue_count: usize,
        requeue_futex: &Arc<Futex>,
        new_requeue_owner: Option<Arc<Thread>>,
        check_value: bool,
    ) -> Result<usize, ZxError> {
        // Acquire both locks in a consistent order to avoid ABBA deadlock.
        let self_addr = &self.inner as *const _ as usize;
        let other_addr = &requeue_futex.inner as *const _ as usize;
        let (mut inner, mut new_inner) = if self_addr <= other_addr {
            let a = self.inner.lock();
            let b = requeue_futex.inner.lock();
            (a, b)
        } else {
            let b = requeue_futex.inner.lock();
            let a = self.inner.lock();
            (a, b)
        };

        // INV-FUTEX-6: failure must be atomic — capture the state
        // both locks protect, and assert it unchanged at every error
        // return below ("if the call fails for any reason, no changes
        // to ownership for either futex are made").
        #[cfg(feature = "invariant-checks")]
        let inv_pre = (
            inner.owner.as_ref().map(|t| t.id()),
            inner.waiter_queue.len(),
            new_inner.owner.as_ref().map(|t| t.id()),
            new_inner.waiter_queue.len(),
        );
        #[cfg(feature = "invariant-checks")]
        let unchanged = || {
            inner.owner.as_ref().map(|t| t.id()) == inv_pre.0
                && inner.waiter_queue.len() == inv_pre.1
                && new_inner.owner.as_ref().map(|t| t.id()) == inv_pre.2
                && new_inner.waiter_queue.len() == inv_pre.3
        };

        if check_value && self.load_user_value() != current_value {
            inv!(unchanged(), "INV-FUTEX-6: requeue failed after mutation");
            return Err(ZxError::BAD_STATE);
        }
        // Check new_requeue_owner is not blocking on EITHER futex.
        // Matches Fuchsia's check: `blocking_futex_id_ == wake_id ||
        // blocking_futex_id_ == requeue_id`. Uses the thread's tracked
        // blocking address rather than scanning the waiter queues,
        // which avoids false positives from stale waiter entries that
        // haven't been cleaned up yet (e.g., after a timeout race).
        if let Some(ref owner) = new_requeue_owner {
            let owner_futex = owner.blocking_futex_addr();
            if owner_futex == self.user_addr || owner_futex == requeue_futex.user_addr {
                inv!(unchanged(), "INV-FUTEX-6: requeue failed after mutation");
                return Err(ZxError::INVALID_ARGS);
            }
        }
        // Collect waiters to wake under the lock, wake after release
        // to avoid ABBA deadlock (see wake() comment).
        let mut waiters_to_wake = alloc::vec::Vec::new();
        for _ in 0..wake_count {
            if let Some(waiter) = inner.waiter_queue.pop_front() {
                waiters_to_wake.push(waiter);
            } else {
                break;
            }
        }
        let woken = waiters_to_wake.len();
        // Collect waiters to requeue under the lock, but do NOT call
        // reset_futex here — that would lock WaiterInner while holding
        // FutexInner, creating an ABBA deadlock with FutexFuture::poll
        // (which locks WaiterInner then FutexInner).
        let requeue_count = requeue_count.min(inner.waiter_queue.len());
        // #862: skip departing waiters — moving a departing waiter to
        // the target queue leaves it lingering there after its Drop
        // (which searches the SOURCE queue), consuming future wakes.
        let mut waiters_to_requeue: alloc::vec::Vec<_> = alloc::vec::Vec::new();
        let mut remaining = requeue_count;
        while remaining > 0 {
            match inner.waiter_queue.pop_front() {
                Some(w) if w.inner.lock().departing => continue,
                Some(w) => {
                    waiters_to_requeue.push(w);
                    remaining -= 1;
                }
                None => break,
            }
        }
        // set owner
        inner.set_owner(None);
        new_inner.set_owner(new_requeue_owner.clone());
        let total = woken + waiters_to_requeue.len();
        // Drop both futex locks before touching waiters to avoid ABBA.
        drop(inner);
        drop(new_inner);
        // Insert waiters into the target queue FIRST, then update their
        // futex reference. This avoids a window where a cancelled future's
        // drop handler can't find the waiter in any queue:
        // - If drop fires before insertion: it looks up the OLD futex
        //   (source), searches that queue — waiter was already drained,
        //   so the search is a harmless no-op.
        // - If drop fires after insertion but before reset_futex: same
        //   as above — drop looks up the OLD futex, not the target.
        // - If drop fires after reset_futex: it looks up the target
        //   futex, finds the waiter in the target queue, removes it.
        {
            let mut new_inner = requeue_futex.inner.lock();
            for waiter in &waiters_to_requeue {
                new_inner.waiter_queue.push_back(waiter.clone());
            }
            // Recompute PI boost for the target owner now that
            // requeued waiters are in the queue. set_owner was called
            // before insertion so it missed these waiters.
            if let Some(ref owner) = new_inner.owner {
                let max_pri = new_inner.max_waiter_priority();
                if max_pri > 0 {
                    owner.boost_priority(max_pri);
                }
            }
        }
        // Now safe to lock WaiterInner (no FutexInner held).
        for waiter in &waiters_to_requeue {
            waiter.reset_futex(requeue_futex.clone());
        }
        for waiter in &waiters_to_wake {
            waiter.wake();
        }
        Ok(total)
    }
}

impl FutexInner {
    fn is_valid_new_owner(&self, new_owner: &Option<Arc<Thread>>, futex_addr: usize) -> bool {
        if let Some(new_owner) = &new_owner {
            // Reject if the new owner is currently blocking on this futex.
            // Uses the thread's tracked blocking address, matching
            // Fuchsia's `blocking_futex_id_` check.
            if new_owner.blocking_futex_addr() == futex_addr {
                return false;
            }
        }
        true
    }

    fn set_owner(&mut self, owner: Option<Arc<Thread>>) {
        // Priority inheritance: when ownership changes, update boosts.
        // Clear the old owner's PI boost.
        if let Some(ref old_owner) = self.owner {
            old_owner.clear_pi_boost();
        }
        // If there's a new owner and waiters in the queue, boost
        // the new owner to the max priority of all waiters.
        if let Some(ref new_owner) = owner {
            let max_waiter_pri = self.max_waiter_priority();
            if max_waiter_pri > 0 {
                new_owner.boost_priority(max_waiter_pri);
            }
        }
        self.owner = owner;
    }

    /// Return the highest priority among all threads in the waiter queue.
    fn max_waiter_priority(&self) -> usize {
        self.waiter_queue
            .iter()
            .filter_map(|w| w.thread.as_ref())
            .map(|t| t.effective_priority())
            .max()
            .unwrap_or(0)
    }
}

pub struct Waiter {
    /// The thread waiting on the futex.
    thread: Option<Arc<Thread>>,
    pub(super) inner: IrqLock<WaiterInner>,
}

pub struct WaiterInner {
    /// The waker of waiting future. `None` indicates first poll.
    waker: Option<Waker>,
    woken: bool,
    /// Set by FutexFuture::drop before the waiter leaves the queue
    /// (#862): a concurrent Futex::wake that pops this waiter must
    /// skip it and wake the next queued waiter instead — otherwise a
    /// departing waiter (BAD_STATE retry) consumes a wake-count slot
    /// and, for wake(1) callers (mutex unlock), the wake is lost.
    /// Matches Linux's futex wake loop skipping FUTEX_STATE_EXITING
    /// waiters.
    departing: bool,
    futex: Arc<Futex>,
}

impl Waiter {
    /// Wake up the waiting thread.
    fn wake(&self) {
        // 5F-2: graph-checked waiter acquisition. Called from
        // Futex::wake() AFTER the futex lock is released (the
        // collect-then-act pattern). Enters from Unlocked or from a
        // FutexInnerLock context — both legal (FutexInner → WaiterInner
        // is the declared edge).
        let mut locked = lock::lockgraph::Locked::new(self);
        let mut inner = locked.lock_and::<super::lock_order::WaiterInnerLock>().0;
        inner.woken = true;
        // The waker may not be set yet if wake() races with the
        // first poll of FutexFuture (between dropping the futex lock
        // and storing the waker). In that case, woken=true is enough
        // — the next poll will see it and return Ready.
        if let Some(waker) = inner.waker.as_ref() {
            waker.wake_by_ref();
        }
    }

    /// Reset futex on requeue. Also updates the thread's
    /// `blocking_futex_addr` to match the new futex, so that
    /// `futex_requeue`'s owner validation (which checks the thread's
    /// blocking address, not the queue contents) sees the correct value.
    ///
    /// Only updates the tracking if the waiter is still active (not
    /// woken or cancelled). If `FutexFuture::drop` already cleared
    /// the tracking, we must not restore it.
    fn reset_futex(&self, futex: Arc<Futex>) {
        let new_addr = futex.user_addr;
        // 5F-2: graph-checked waiter acquisition (leaf).
        let mut locked = lock::lockgraph::Locked::new(self);
        let mut inner = locked.lock_and::<super::lock_order::WaiterInnerLock>().0;
        let still_active = !inner.woken;
        inner.futex = futex;
        drop(inner);
        if still_active {
            if let Some(ref thread) = self.thread {
                thread.set_blocking_futex(new_addr);
            }
        }
    }
}

#[cfg(test)]
mod tests {}
