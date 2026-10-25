use crate::{object::*, task::Thread};
use alloc::collections::VecDeque;
use alloc::sync::Arc;
use core::future::Future;
use core::pin::Pin;
use core::sync::atomic::*;
use core::task::{Context, Poll, Waker};
use lock::Mutex;

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
    inner: Mutex<FutexInner>,
}

impl_kobject!(Futex);

#[derive(Default)]
struct FutexInner {
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
            inner: Mutex::new(FutexInner::default()),
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
        // Collect waiters under the futex lock, then wake them after
        // releasing it. This avoids an ABBA deadlock: wake() holds
        // FutexInner then WaiterInner, while FutexFuture::poll() holds
        // WaiterInner then FutexInner.
        let waiters: alloc::vec::Vec<_> = {
            let mut inner = self.inner.lock();
            inner.set_owner(None);
            let mut collected = alloc::vec::Vec::new();
            for _ in 0..wake_count {
                if let Some(waiter) = inner.waiter_queue.pop_front() {
                    collected.push(waiter);
                } else {
                    break;
                }
            }
            collected
        };
        let count = waiters.len();
        for waiter in &waiters {
            waiter.wake();
        }
        count
    }

    // ------ Advanced APIs on Zircon ------

    /// Get the owner of the futex.
    pub fn owner(&self) -> Option<Arc<Thread>> {
        self.inner.lock().owner.clone()
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
            waiter: Arc<Waiter>,
            current_value: i32,
            new_owner: Option<Arc<Thread>>,
        }
        impl Future for FutexFuture {
            type Output = ZxResult;

            fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
                let mut inner = self.waiter.inner.lock();
                // check wakeup
                if inner.woken {
                    return Poll::Ready(Ok(()));
                }
                // first time?
                if inner.waker.is_none() {
                    // Acquire the futex lock BEFORE the value check.
                    // The value check and waiter registration must be
                    // atomic with respect to futex_wake — if a wake
                    // could occur between the check and the push, a
                    // waiter would be orphaned (never woken). See
                    // Fuchsia's futex_context.cc: "Those two steps must
                    // together be atomic with respect to FutexWake()."
                    let mut futex = inner.futex.inner.lock();
                    // check value under the lock
                    let value = inner.futex.load_user_value();
                    if value != self.current_value {
                        return Poll::Ready(Err(ZxError::BAD_STATE));
                    }
                    // check new owner
                    if !futex.is_valid_new_owner(&self.new_owner, inner.futex.user_addr) {
                        return Poll::Ready(Err(ZxError::INVALID_ARGS));
                    }
                    // Set the new owner immediately when the wait begins,
                    // not when the waiter wakes. This matches Fuchsia's
                    // behavior where get_owner reflects the owner set by
                    // the most recent successful futex_wait.
                    futex.set_owner(self.new_owner.clone());
                    futex.waiter_queue.push_back(self.waiter.clone());
                    // Priority inheritance: if the new waiter has
                    // higher priority than the owner, boost the owner.
                    if let Some(ref owner) = futex.owner {
                        if let Some(ref waiter_thread) = self.waiter.thread {
                            let waiter_pri = waiter_thread.effective_priority();
                            if waiter_pri > owner.effective_priority() {
                                owner.boost_priority(waiter_pri);
                            }
                        }
                    }
                    // Track which futex this thread is blocking on,
                    // so futex_requeue's owner validation can check
                    // without scanning the waiter queue.
                    if let Some(ref thread) = self.waiter.thread {
                        thread.set_blocking_futex(inner.futex.user_addr);
                    }
                    drop(futex);
                    inner.waker.replace(cx.waker().clone());
                }
                Poll::Pending
            }
        }
        // The FutexFuture will be dropped when the thread is no longer waiting
        // if we wake without be woken, remove myself from the waiter_queue
        impl Drop for FutexFuture {
            fn drop(&mut self) {
                let inner = self.waiter.inner.lock();
                if !inner.woken {
                    let futex_arc = inner.futex.clone();
                    let mut futex = futex_arc.inner.lock();
                    if let Some(pos) = futex
                        .waiter_queue
                        .iter()
                        .position(|x| Arc::ptr_eq(x, &self.waiter))
                    {
                        futex.waiter_queue.swap_remove_back(pos);
                    }
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
                // Clear blocking futex tracking.
                if let Some(ref thread) = self.waiter.thread {
                    thread.set_blocking_futex(0);
                }
            }
        }
        FutexFuture {
            waiter: Arc::new(Waiter {
                thread,
                inner: Mutex::new(WaiterInner {
                    waker: None,
                    woken: false,
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
        // Pop waiter and set owner under the lock, then wake after
        // releasing to avoid ABBA deadlock (see wake() comment).
        let (waiter_to_wake, new_owner) = {
            let mut inner = self.inner.lock();
            let popped = inner.waiter_queue.pop_front();
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
    /// with swapped source/target addresses. Note: the broader lock
    /// ordering between `Futex::inner` and `WaiterInner` is a pre-existing
    /// concern not addressed here (see issue #102).
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

        if check_value && self.load_user_value() != current_value {
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
        let waiters_to_requeue: alloc::vec::Vec<_> =
            inner.waiter_queue.drain(..requeue_count).collect();
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

struct Waiter {
    /// The thread waiting on the futex.
    thread: Option<Arc<Thread>>,
    inner: Mutex<WaiterInner>,
}

struct WaiterInner {
    /// The waker of waiting future. `None` indicates first poll.
    waker: Option<Waker>,
    woken: bool,
    futex: Arc<Futex>,
}

impl Waiter {
    /// Wake up the waiting thread.
    fn wake(&self) {
        let mut inner = self.inner.lock();
        inner.woken = true;
        inner.waker.as_ref().unwrap().wake_by_ref();
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
        let mut inner = self.inner.lock();
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
mod tests {
    use super::*;
    use crate::task::{Job, Process};
    use core::time::Duration;

    #[async_std::test]
    async fn wait() {
        static VALUE: AtomicI32 = AtomicI32::new(1);
        let futex = Futex::new(&VALUE as *const AtomicI32 as usize);

        let count = futex.wake(1);
        assert_eq!(count, 0);

        // inconsistent value should fail.
        assert_eq!(futex.wait(0).await, Err(ZxError::BAD_STATE));

        // spawn a new task to wake me up.
        {
            let futex = futex.clone();
            async_std::task::spawn(async move {
                async_std::task::sleep(Duration::from_millis(10)).await;
                VALUE.store(2, Ordering::SeqCst);
                let count = futex.wake(1);
                assert_eq!(count, 1);
            });
        }
        // wait for wake.
        futex.wait(1).await.unwrap();
        assert_eq!(VALUE.load(Ordering::SeqCst), 2);
        assert_eq!(futex.wake(1), 0);
    }

    #[async_std::test]
    async fn requeue() {
        static VALUE: AtomicI32 = AtomicI32::new(1);
        let futex = Futex::new(&VALUE as *const AtomicI32 as usize);
        static REQUEUE_VALUE: AtomicI32 = AtomicI32::new(100);
        let requeue_futex = Futex::new(&REQUEUE_VALUE as *const AtomicI32 as usize);

        let count = futex.wake(1);
        assert_eq!(count, 0);

        // inconsistent value should fail.
        assert_eq!(futex.wait(0).await, Err(ZxError::BAD_STATE));

        // spawn a new task to wait
        {
            let futex = futex.clone();
            async_std::task::spawn(async move {
                futex.wait(1).await.unwrap();
            });
        }
        // spawn a new task to requeue.
        {
            let futex = futex.clone();
            async_std::task::spawn(async move {
                async_std::task::sleep(Duration::from_millis(10)).await;
                VALUE.store(2, Ordering::SeqCst);

                let waiters = futex.inner.lock().waiter_queue.clone();
                assert_eq!(waiters.len(), 2);

                // inconsistent value should fail.
                assert_eq!(
                    futex.requeue(1, 1, 1, &requeue_futex, None, true),
                    Err(ZxError::BAD_STATE)
                );
                assert!(futex.requeue(2, 1, 1, &requeue_futex, None, true).is_ok());
                // 1 waiter waken, 1 waiter moved into `requeue_futex`.
                assert_eq!(futex.inner.lock().waiter_queue.len(), 0);
                assert_eq!(requeue_futex.inner.lock().waiter_queue.len(), 1);
                assert!(Arc::ptr_eq(
                    &requeue_futex.inner.lock().waiter_queue[0],
                    &waiters[1]
                ));
                // wake the requeued waiter.
                assert_eq!(requeue_futex.wake(1), 1);
            });
        }
        // wait for wake.
        futex.wait(1).await.unwrap();
        assert_eq!(VALUE.load(Ordering::SeqCst), 2);
    }

    #[async_std::test]
    async fn owner() {
        let root_job = Job::root();
        let proc = Process::create(&root_job, "proc").expect("failed to create process");
        let thread = Thread::create(&proc, "thread").expect("failed to create thread");

        static VALUE: AtomicI32 = AtomicI32::new(1);
        let futex = proc.get_futex(&VALUE as *const AtomicI32 as usize);
        assert!(futex.owner().is_none());
        futex.inner.lock().set_owner(Some(thread.clone()));

        {
            let futex = futex.clone();
            let thread = thread.clone();
            async_std::task::spawn(async move {
                futex
                    .wait_with_owner(1, Some(thread.clone()), Some(thread))
                    .await
                    .unwrap();
            });
        }
        async_std::task::sleep(Duration::from_millis(10)).await;
        assert_eq!(
            futex
                .wait_with_owner(1, Some(thread.clone()), Some(thread.clone()))
                .await
                .unwrap_err(),
            ZxError::INVALID_ARGS
        );

        futex.inner.lock().set_owner(None);
        futex.wake_single_owner();
        assert!(Arc::ptr_eq(&futex.owner().unwrap(), &thread));
        assert_eq!(futex.wake(1), 0);
        proc.exit(0);
    }

    #[async_std::test]
    async fn requeue_single_owner() {
        // Test requeue with ownership transfer: wake exactly 1 waiter and
        // move others to the requeue target, setting the new owner.
        let root_job = Job::root();
        let proc = Process::create(&root_job, "proc").expect("failed to create process");
        let thread1 = Thread::create(&proc, "t1").expect("failed to create thread");
        let thread2 = Thread::create(&proc, "t2").expect("failed to create thread");
        // Owner thread is NOT a waiter — matches Fuchsia semantics where
        // the requeue owner must not be waiting on either futex.
        let owner_thread = Thread::create(&proc, "owner").expect("failed to create thread");

        static VALUE: AtomicI32 = AtomicI32::new(1);
        let futex = proc.get_futex(&VALUE as *const AtomicI32 as usize);
        static REQUEUE_VALUE: AtomicI32 = AtomicI32::new(0);
        let requeue_futex = proc.get_futex(&REQUEUE_VALUE as *const AtomicI32 as usize);

        // Two waiters on the main futex
        {
            let futex = futex.clone();
            let thread1 = thread1.clone();
            async_std::task::spawn(async move {
                let _ = futex.wait_with_owner(1, Some(thread1), None).await;
            });
        }
        {
            let futex = futex.clone();
            let thread2 = thread2.clone();
            async_std::task::spawn(async move {
                let _ = futex.wait_with_owner(1, Some(thread2), None).await;
            });
        }

        // Poll until both waiters have entered the queue (bounded to avoid hanging).
        for _ in 0..200 {
            if futex.inner.lock().waiter_queue.len() == 2 {
                break;
            }
            async_std::task::sleep(Duration::from_millis(1)).await;
        }
        assert_eq!(futex.inner.lock().waiter_queue.len(), 2);

        // A waiter thread as owner should be rejected (Fuchsia validates
        // before wake/requeue).
        assert_eq!(
            futex.requeue(1, 1, 1, &requeue_futex, Some(thread1.clone()), true),
            Err(ZxError::INVALID_ARGS)
        );

        // Requeue: wake 1, move 1 to requeue_futex, set non-waiter as owner
        futex
            .requeue(1, 1, 1, &requeue_futex, Some(owner_thread.clone()), true)
            .unwrap();

        // 1 woken, 1 requeued
        assert_eq!(futex.inner.lock().waiter_queue.len(), 0);
        assert_eq!(requeue_futex.inner.lock().waiter_queue.len(), 1);

        // The requeue futex should have the new owner set
        assert!(requeue_futex.owner().is_some());
        assert!(Arc::ptr_eq(&requeue_futex.owner().unwrap(), &owner_thread));

        // Wake the requeued waiter to clean up
        requeue_futex.wake(1);
        proc.exit(0);
    }

    /// Regression test: futex_wake must not race with futex_wait's
    /// value-check-and-register sequence. If the value check and queue
    /// push are not atomic (under the same lock), a wake can drain the
    /// queue between the check and push, orphaning the waiter forever.
    #[async_std::test]
    async fn wake_during_registration_race() {
        for _ in 0..100 {
            static VALUE: AtomicI32 = AtomicI32::new(0);
            VALUE.store(1, Ordering::SeqCst);
            let futex = Futex::new(&VALUE as *const AtomicI32 as usize);

            let futex_w = futex.clone();
            let waiter = async_std::task::spawn(async move { futex_w.wait(1).await });

            async_std::task::yield_now().await;

            VALUE.store(2, Ordering::SeqCst);
            futex.wake(u32::MAX as usize);

            // Must complete: Ok (woken) or BAD_STATE (value changed).
            // Use a timeout so the test fails clearly if the race regresses.
            let result = async_std::future::timeout(Duration::from_millis(500), waiter).await;
            assert!(result.is_ok(), "waiter hung — lost wakeup (futex race)");
        }
    }
}
