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
            inner: IrqLock::new(FutexInner {
                waiter_queue: VecDeque::new(),
                owner: None,
            }),
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

    // ------ Advanced APIs on Zircon ------

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
            waiter: Arc<Waiter>,
            current_value: i32,
            new_owner: Option<Arc<Thread>>,
        }
        impl Future for FutexFuture {
            type Output = ZxResult;

            fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
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
                    let inner = locked.lock_and::<super::lock_order::WaiterInnerLock>().0;
                    if inner.woken {
                        // Already woken — wake() popped us from the
                        // queue. Just clear tracking.
                        if let Some(ref thread) = self.waiter.thread {
                            thread.set_blocking_futex(0);
                        }
                        return;
                    }
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
                if let Some(pos) = futex
                    .waiter_queue
                    .iter()
                    .position(|x| alloc::sync::Arc::ptr_eq(x, &self.waiter))
                {
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
                // Clear blocking futex tracking.
                if let Some(ref thread) = self.waiter.thread {
                    thread.set_blocking_futex(0);
                }
            }
        }
        FutexFuture {
            waiter: Arc::new(Waiter {
                thread,
                inner: IrqLock::new(WaiterInner {
                    waker: None,
                    woken: false,
                    futex: self.clone(),
                }),
            }),
            current_value,
            new_owner,
        }
    }
}

impl FutexInner {
    /// Return the highest priority among all threads in the waiter queue.
    ///
    /// Unverified (#833 Phase 1): used only for priority-inheritance
    /// boosting, which has no effect on the futex model (queue
    /// contents, owner). See the trust-base notes in the verus! block
    /// below.
    fn max_waiter_priority(&self) -> usize {
        self.waiter_queue
            .iter()
            .filter_map(|w| w.thread.as_ref())
            .map(|t| t.effective_priority())
            .max()
            .unwrap_or(0)
    }
}

pub struct WaiterInner {
    /// The waker of waiting future. `None` indicates first poll.
    waker: Option<Waker>,
    woken: bool,
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

// ── Formal verification (#833 Phase 1) ─────────────────────────────────
//
// Everything in this `verus!` block is the REAL production code: under
// normal rustc the macro erases the spec/proof annotations and compiles
// the function bodies unchanged; `make verify` (cargo-verus) machine-
// checks the contracts against those same bodies. There is no separate
// model — a change to any body below that breaks a contract fails
// verification.
//
// ## Structure
//
// - **Cores** (`FutexInner::collect_wake_waiters`, `pop_single_owner_waiter`,
//   `requeue_core`, `insert_requeued`, `set_owner`, `is_valid_new_owner`):
//   the critical-section logic of `wake`/`wake_single_owner`/`requeue`,
//   fully verified.
// - **Wrappers** (`wake`, `wake_single_owner`, `requeue`): verified
//   compositions of the cores; the lock acquisition itself is delegated
//   to small `external_body` glue helpers.
// - **Proofs**: the task-5 properties, as lemmas over the core contracts.
//
// ## Trust base (everything not verified, each item auditable)
//
// 1. **Queue axioms** (TRUSTED below): `VecDeque::pop_front`/`push_back`
//    length semantics. `VecDeque::len` is used as an uninterpreted spec
//    function; the axioms are the only facts about it.
// 2. **Lock glue** (TRUSTED below): `run_collect_wake_under_lock`,
//    `run_pop_single_owner_under_lock`, `run_requeue_source_under_locks`,
//    `run_insert_requeued_under_lock` acquire the IrqLock(s) and run the
//    verified cores. Trust: an IrqLock guard grants exclusive access for
//    the duration of the critical section (single-threaded semantics
//    inside), and the glue passes arguments through unchanged.
// 3. **Thread methods** (`clear_pi_boost`, `boost_priority`,
//    `effective_priority`, `blocking_futex_addr`, `set_blocking_futex`):
//    opaque; they mutate thread state only, never a `FutexInner`.
// 4. **`Waiter::wake`/`reset_futex`**: take only `WaiterInner`, never a
//    `FutexInner`.
// 5. **`load_user_value`**: unsafe SMAP read of user memory, opaque.
// 6. **Async wait path** (`wait`, `wait_with_owner`, `FutexFuture`):
//    outside Verus's supported subset (no async/await); not verified in
//    this phase. The registration logic it shares with the verified
//    owner-validation (`is_valid_new_owner`) is verified; the future
//    machinery is covered by the #850 race test and core-tests.
verus! {

// ── Opaque type specs (trust base items 3-5) ──────────────────────────
//
// These types appear in fields of the verus-visible structs but are
// never inspected by the futex specs; declaring them here makes them
// opaque Verus-known types. `Thread` methods called from verified code
// (`blocking_futex_addr`, `clear_pi_boost`, `boost_priority`) are
// uninterpreted: they mutate only thread state, never a `FutexInner`.
//
// All are `verus_only`: they exist solely for verification and are
// invisible to normal rustc builds (some reference unstable trait
// paths that only the Verus toolchain accepts).

#[cfg(verus_only)]
#[verifier::external_type_specification]
#[verifier::external_body]
#[allow(dead_code)]
pub struct ExThread(Thread);

#[cfg(verus_only)]
#[verifier::external_type_specification]
#[verifier::external_body]
#[allow(dead_code)]
pub struct ExWaiterInner(WaiterInner);

#[cfg(verus_only)]
#[verifier::reject_recursive_types(R)]
#[verifier::reject_recursive_types(T)]
#[verifier::external_type_specification]
#[verifier::external_body]
#[allow(dead_code)]
pub struct ExMutex<T, R>(spin::mutex::Mutex<T, R>)
where
    T: core::marker::MetaSized + ?Sized,;

#[cfg(verus_only)]
#[verifier::external_type_specification]
#[verifier::external_body]
#[allow(dead_code)]
pub struct ExSpin(spin::relax::Spin);

#[cfg(verus_only)]
#[verifier::external_type_specification]
#[allow(dead_code)]
pub struct ExZxError(crate::error::ZxError);

#[cfg(verus_only)]
#[verifier::external_type_specification]
#[verifier::external_body]
#[allow(dead_code)]
pub struct ExFutex(Futex);

// ── Opaque function specs (trust base items 3-5) ──────────────────────
//
// These functions are called from verified code but are themselves
// unverified. Each is opaque (no assumed postcondition beyond its
// type): the trust argument for each is documented in the trust base
// above — thread-state-only effects (never a `FutexInner`), and
// waiter-only effects (never a `FutexInner`).

/// TRUSTED axiom: a thread's tracked blocking-futex address. Linked
/// to the exec `Thread::blocking_futex_addr` by the assume_spec below;
/// specs use this view.
#[cfg(verus_only)]
pub uninterp spec fn spec_blocking_futex_addr(t: &Thread) -> usize;

#[cfg(verus_only)]
pub assume_specification [crate::task::Thread::blocking_futex_addr](
    _0: &Thread,
) -> (result: usize)
    ensures
        result == spec_blocking_futex_addr(_0);

#[cfg(verus_only)]
pub assume_specification [crate::task::Thread::boost_priority](
    _0: &Thread,
    _1: usize,
);

#[cfg(verus_only)]
pub assume_specification [crate::task::Thread::clear_pi_boost](_0: &Thread);

#[cfg(verus_only)]
pub assume_specification [crate::signal::futex::FutexInner::max_waiter_priority](
    _0: &FutexInner,
) -> (result: usize);

#[cfg(verus_only)]
pub assume_specification [crate::signal::futex::Futex::load_user_value](_0: &Futex) -> (result: i32);

#[cfg(verus_only)]
pub assume_specification [crate::signal::futex::Waiter::wake](_0: &Waiter);

#[cfg(verus_only)]
pub assume_specification [crate::signal::futex::Waiter::reset_futex](_0: &Waiter, _1: Arc<Futex>);

// ── Struct definitions (verus-visible; erasure emits them unchanged) ──
//
// These must live inside `verus!` for their fields to be visible to
// the specs below. Under normal rustc they compile identically to a
// declaration outside the macro.

/// The mutable state of a futex, guarded by its `IrqLock`.
pub struct FutexInner {
    pub(super) waiter_queue: VecDeque<Arc<Waiter>>,
    /// NOTE: use `set_owner`
    pub(super) owner: Option<Arc<Thread>>,
}

/// A thread blocked on (or recently woken from) a futex wait.
pub struct Waiter {
    /// The thread waiting on the futex.
    pub(super) thread: Option<Arc<Thread>>,
    pub(super) inner: IrqLock<WaiterInner>,
}

// ── TRUSTED: waiter-queue axioms ──────────────────────────────────────
// `VecDeque::len` appears in specs as an uninterpreted function; these
// one-line wrappers (the only assumed facts about the queue) say:
// pop_front either removes exactly one waiter or reports (and means)
// an empty queue; push_back adds exactly one.

/// TRUSTED: pop the front waiter. Axiomatized: on `Some`, the queue
/// length drops by exactly one; on `None`, the queue was and is empty.
#[verifier::external_body]
pub(super) fn queue_pop_front(
    queue: &mut VecDeque<Arc<Waiter>>,
) -> (result: Option<Arc<Waiter>>)
    ensures
        match result {
            Some(_) => queue_len_raw(final(queue)) + 1 == queue_len_raw(old(queue)),
            None => queue_len_raw(final(queue)) == 0 && queue_len_raw(old(queue)) == 0,
        }
{
    queue.pop_front()
}

/// TRUSTED: append a waiter to the back. Axiomatized: the queue
/// length grows by exactly one.
#[verifier::external_body]
pub(super) fn queue_push_back(queue: &mut VecDeque<Arc<Waiter>>, waiter: Arc<Waiter>)
    ensures
        queue_len_raw(final(queue)) == queue_len_raw(old(queue)) + 1,
{
    queue.push_back(waiter)
}

/// TRUSTED axiom: the length of a waiter queue, as an uninterpreted
/// spec function. `queue_pop_front`/`queue_push_back`'s ensures (the
/// only axioms about it) define its behavior relative to those ops.
pub(super) uninterp spec fn queue_len_raw(queue: &VecDeque<Arc<Waiter>>) -> usize;

/// Spec-level queue length of a `FutexInner`.
pub(super) open spec fn queue_len(inner: &FutexInner) -> usize {
    queue_len_raw(&inner.waiter_queue)
}

/// The two waiter sets produced by `requeue_core`: waiters to wake on
/// the source futex and waiters to move to the target futex.
type RequeueSets = Result<
    (alloc::vec::Vec<Arc<Waiter>>, alloc::vec::Vec<Arc<Waiter>>),
    ZxError,
>;

// ── Verified cores ────────────────────────────────────────────────────

impl FutexInner {
    /// Verified core of `Futex::wake`: clear the owner and collect up
    /// to `wake_count` waiters from the front of the queue.
    ///
    /// Zircon contract (zx_futex_wake): wakes at most `wake_count`
    /// waiters; "the owner of the futex is set to nothing, regardless
    /// of the wake count". No waiter is lost: every waiter removed
    /// from the queue is returned for waking (conservation).
    pub(super) fn collect_wake_waiters(
        &mut self,
        wake_count: usize,
    ) -> (collected: alloc::vec::Vec<Arc<Waiter>>)
        ensures
            collected.len() <= wake_count,
            queue_len(final(self)) + collected.len() == queue_len(old(self)),
            final(self).owner is None,
    {
        self.set_owner(None);
        let mut collected: alloc::vec::Vec<Arc<Waiter>> = alloc::vec::Vec::new();
        let mut i: usize = 0;
        while i < wake_count
            invariant
                i <= wake_count,
                collected.len() == i,
                collected.len() + queue_len(self) == queue_len(old(self)),
            decreases wake_count - i,
        {
            if let Some(waiter) = queue_pop_front(&mut self.waiter_queue) {
                collected.push(waiter);
            } else {
                break;
            }
            i += 1;
        }
        collected
    }

    /// Verified core of `Futex::wake_single_owner`: pop one waiter and
    /// set the owner to that waiter's thread.
    ///
    /// Zircon contract (zx_futex_wake_single_owner): "If there is at
    /// least one thread to wake, the owner of the futex will be set to
    /// the thread which was woken. Otherwise, the futex will have no
    /// owner."
    pub(super) fn pop_single_owner_waiter(&mut self) -> (popped: Option<Arc<Waiter>>)
        ensures
            match popped {
                Some(w) => {
                    &&& queue_len(final(self)) + 1 == queue_len(old(self))
                    &&& final(self).owner == w.thread
                },
                None => {
                    &&& queue_len(final(self)) == queue_len(old(self))
                    &&& queue_len(old(self)) == 0
                    &&& final(self).owner is None
                },
            },
    {
        let popped = queue_pop_front(&mut self.waiter_queue);
        let owner: Option<Arc<Thread>> = match &popped {
            Some(w) => w.thread.clone(),
            None => None,
        };
        self.set_owner(owner);
        popped
    }

    /// Verified core of `Futex::requeue` (source-futex side): the
    /// value/owner validations, then the wake/requeue collections and
    /// the source ownership transition.
    ///
    /// `value_ok` is the user-memory value check (performed by the
    /// wrapper, which reads user space). The owner validation runs
    /// here: the new requeue owner must not be blocking on either
    /// futex (Zircon: `ZX_ERR_INVALID_ARGS`).
    ///
    /// Zircon contract (zx_futex_requeue): "if the call fails for any
    /// reason, no changes to ownership for either futex are made" —
    /// the `Err` case is atomic. On success, waiters are partitioned
    /// into woken and requeued with none lost, and the source owner is
    /// cleared.
    #[allow(clippy::type_complexity)]
    pub(super) fn requeue_core(
        &mut self,
        value_ok: bool,
        new_requeue_owner: &Option<Arc<Thread>>,
        self_user_addr: usize,
        requeue_user_addr: usize,
        wake_count: usize,
        requeue_count: usize,
    ) -> (result: RequeueSets)
        ensures
            match result {
                // Failure is atomic: no ownership or queue change.
                Err(_) => {
                    &&& final(self).owner == old(self).owner
                    &&& queue_len(final(self)) == queue_len(old(self))
                },
                // Success: bounds, conservation, source owner cleared.
                Ok((wake_set, requeue_set)) => {
                    &&& wake_set.len() <= wake_count
                    &&& requeue_set.len() <= requeue_count
                    &&& queue_len(final(self)) + wake_set.len() + requeue_set.len()
                        == queue_len(old(self))
                    &&& final(self).owner is None
                },
            },
    {
        // Value check (result passed in — reads user memory).
        if !value_ok {
            return Err(ZxError::BAD_STATE);
        }
        // Owner validation: reject owners blocking on either futex.
        // Matches Fuchsia's `blocking_futex_id_` check.
        if let Some(ref owner) = new_requeue_owner {
            let owner_futex = owner.blocking_futex_addr();
            if owner_futex == self_user_addr || owner_futex == requeue_user_addr {
                return Err(ZxError::INVALID_ARGS);
            }
        }
        // Collect waiters to wake (collect-then-act; see wake()).
        let mut waiters_to_wake: alloc::vec::Vec<Arc<Waiter>> = alloc::vec::Vec::new();
        let mut i: usize = 0;
        while i < wake_count
            invariant
                i <= wake_count,
                waiters_to_wake.len() == i,
                waiters_to_wake.len() + queue_len(self) == queue_len(old(self)),
            decreases wake_count - i,
        {
            if let Some(waiter) = queue_pop_front(&mut self.waiter_queue) {
                waiters_to_wake.push(waiter);
            } else {
                break;
            }
            i += 1;
        }
        // Collect waiters to requeue. reset_futex is NOT called here —
        // that would lock WaiterInner while holding FutexInner (the
        // #102 ABBA); the wrapper does it after the locks are released.
        let mut waiters_to_requeue: alloc::vec::Vec<Arc<Waiter>> = alloc::vec::Vec::new();
        let mut j: usize = 0;
        while j < requeue_count
            invariant
                j <= requeue_count,
                waiters_to_requeue.len() == j,
                waiters_to_wake.len() + waiters_to_requeue.len() + queue_len(self)
                    == queue_len(old(self)),
            decreases requeue_count - j,
        {
            if let Some(waiter) = queue_pop_front(&mut self.waiter_queue) {
                waiters_to_requeue.push(waiter);
            } else {
                break;
            }
            j += 1;
        }
        self.set_owner(None);
        Ok((waiters_to_wake, waiters_to_requeue))
    }

    /// Verified core (target-futex side of requeue): insert the
    /// requeued waiters into the target queue and re-boost the target
    /// owner (priority inheritance).
    pub(super) fn insert_requeued(&mut self, waiters: &alloc::vec::Vec<Arc<Waiter>>)
        ensures
            queue_len(final(self)) == queue_len(old(self)) + waiters.len(),
            final(self).owner == old(self).owner,
    {
        let mut i: usize = 0;
        while i < waiters.len()
            invariant
                i <= waiters.len(),
                queue_len(self) == queue_len(old(self)) + i,
            decreases waiters.len() - i,
        {
            queue_push_back(&mut self.waiter_queue, waiters[i].clone());
            i += 1;
        }
        if let Some(ref owner) = self.owner {
            let max_pri = self.max_waiter_priority();
            if max_pri > 0 {
                owner.boost_priority(max_pri);
            }
        }
    }

    /// Verified: ownership transition. The priority-inheritance calls
    /// mutate only thread state (trusted, see trust base); the futex
    /// model sees `owner` written and the queue untouched.
    pub(super) fn set_owner(&mut self, owner: Option<Arc<Thread>>)
        ensures
            final(self).owner == owner,
            queue_len(final(self)) == queue_len(old(self)),
    {
        // Clear the old owner's PI boost.
        if let Some(ref old_owner) = self.owner {
            old_owner.clear_pi_boost();
        }
        // Boost the new owner to the max waiter priority.
        if let Some(ref new_owner) = owner {
            let max_waiter_pri = self.max_waiter_priority();
            if max_waiter_pri > 0 {
                new_owner.boost_priority(max_waiter_pri);
            }
        }
        self.owner = owner;
    }

    /// Verified: owner validation for `zx_futex_wait` (and requeue).
    /// Zircon: `INVALID_ARGS` if the new owner is currently a member
    /// of the waiters for this futex — checked via the thread's
    /// tracked blocking address (matching Fuchsia's
    /// `blocking_futex_id_`).
    pub(super) fn is_valid_new_owner(
        &self,
        new_owner: &Option<Arc<Thread>>,
        futex_addr: usize,
    ) -> (result: bool)
        ensures
            result == match new_owner {
                Some(t) => spec_blocking_futex_addr(t) != futex_addr,
                None => true,
            },
    {
        if let Some(new_owner) = &new_owner {
            if new_owner.blocking_futex_addr() == futex_addr {
                return false;
            }
        }
        true
    }
}

// ── Verified wrappers + trusted lock glue ─────────────────────────────

impl Futex {
    /// TRUSTED glue: acquire the futex lock (lockgraph-checked,
    /// collect-then-act per 5F-2) and run the verified
    /// `collect_wake_waiters` core. Trust: the IrqLock grants
    /// exclusive `&mut FutexInner` for the critical section.
    #[verifier::external_body]
    fn run_collect_wake_under_lock(
        &self,
        wake_count: usize,
    ) -> (waiters: alloc::vec::Vec<Arc<Waiter>>)
        ensures
            waiters.len() <= wake_count,
    {
        let mut locked = lock::lockgraph::Locked::new(self);
        let mut inner = locked.lock_and::<super::lock_order::FutexInnerLock>().0;
        inner.collect_wake_waiters(wake_count)
    }

    /// TRUSTED glue: same pattern for `wake_single_owner`.
    #[verifier::external_body]
    fn run_pop_single_owner_under_lock(&self) -> (popped: Option<Arc<Waiter>>)
        ensures
            true, // the observable contract lives on the core
    {
        let mut locked = lock::lockgraph::Locked::new(self);
        let mut inner = locked.lock_and::<super::lock_order::FutexInnerLock>().0;
        inner.pop_single_owner_waiter()
    }

    /// TRUSTED glue: the dual-lock critical section of requeue (both
    /// futex locks acquired in address order to prevent ABBA between
    /// concurrent requeues with swapped source/target). Runs the
    /// verified source core, then the target ownership transition.
    /// On `Err`, neither futex is mutated (the core is atomic; the
    /// target `set_owner` runs only after a successful core call).
    #[verifier::external_body]
    #[allow(clippy::type_complexity)]
    fn run_requeue_source_under_locks(
        &self,
        requeue_futex: &Arc<Futex>,
        value_ok: bool,
        new_requeue_owner: &Option<Arc<Thread>>,
        wake_count: usize,
        requeue_count: usize,
    ) -> (result: RequeueSets)
        ensures
            match result {
                Err(_) => true,
                Ok((wake_set, requeue_set)) => {
                    &&& wake_set.len() <= wake_count
                    &&& requeue_set.len() <= requeue_count
                },
            },
    {
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

        let result = inner.requeue_core(
            value_ok,
            new_requeue_owner,
            self.user_addr,
            requeue_futex.user_addr,
            wake_count,
            requeue_count,
        )?;
        // Target ownership transition (source owner was cleared in
        // the core). Only reached when the core succeeded, so failure
        // leaves both futexes untouched.
        new_inner.set_owner(new_requeue_owner.clone());
        Ok(result)
    }

    /// TRUSTED glue: re-acquire the target futex lock and run the
    /// verified `insert_requeued` core.
    #[verifier::external_body]
    fn run_insert_requeued_under_lock(
        &self,
        requeue_futex: &Arc<Futex>,
        waiters: &alloc::vec::Vec<Arc<Waiter>>,
    ) {
        let mut new_inner = requeue_futex.inner.lock();
        new_inner.insert_requeued(waiters);
    }

    /// Wake some number of threads waiting on a futex.
    ///
    /// It wakes at most `wake_count` of the waiters that are waiting on this futex.
    /// Return the number of waiters that were woken up.
    ///
    /// # Ownership
    ///
    /// The owner of the futex is set to nothing, regardless of the wake count.
    pub fn wake(&self, wake_count: usize) -> (count: usize)
        ensures
            count <= wake_count,
    {
        let waiters: alloc::vec::Vec<_> = self.run_collect_wake_under_lock(wake_count);
        let mut i: usize = 0;
        while i < waiters.len()
            invariant
                i <= waiters.view().len(),
            decreases waiters.view().len() - i,
        {
            waiters[i].wake();
            i += 1;
        }
        waiters.len()
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
    ///
    /// The ownership contract is carried by the verified core
    /// `FutexInner::pop_single_owner_waiter`; it is not expressible on
    /// this wrapper without a ghost view of the futex state (Phase 2).
    pub fn wake_single_owner(&self) {
        let waiter_to_wake = self.run_pop_single_owner_under_lock();
        if let Some(waiter) = waiter_to_wake {
            waiter.wake();
        }
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
    ///
    /// The waiter-partition and atomicity contracts are carried by the
    /// verified core `FutexInner::requeue_core`; the wrapper-level
    /// contract below is what is observable without a ghost futex view.
    pub fn requeue(
        &self,
        current_value: i32,
        wake_count: usize,
        requeue_count: usize,
        requeue_futex: &Arc<Futex>,
        new_requeue_owner: Option<Arc<Thread>>,
        check_value: bool,
    ) -> (result: Result<usize, ZxError>)
        requires
            // Guaranteed by the syscall layer: both counts come from
            // u32s, so their sum fits in usize (64-bit).
            wake_count + requeue_count <= usize::MAX,
        ensures
            match result {
                Err(_) => true,
                Ok(total) => total <= wake_count + requeue_count,
            },
    {
        // Value check reads user memory (unsafe SMAP access) — done
        // here, result passed to the verified core.
        let value_ok = !check_value || self.load_user_value() == current_value;
        let (waiters_to_wake, waiters_to_requeue) = self.run_requeue_source_under_locks(
            requeue_futex,
            value_ok,
            &new_requeue_owner,
            wake_count,
            requeue_count,
        )?;
        let total = waiters_to_wake.len() + waiters_to_requeue.len();
        // Insert waiters into the target queue FIRST, then update their
        // futex reference. This avoids a window where a cancelled future's
        // drop handler can't find the waiter in any queue (see the
        // comments in run_requeue_source_under_locks' callers, 5F-2).
        self.run_insert_requeued_under_lock(requeue_futex, &waiters_to_requeue);
        // Now safe to lock WaiterInner (no FutexInner held).
        let mut i: usize = 0;
        while i < waiters_to_requeue.len()
            invariant
                i <= waiters_to_requeue.len(),
            decreases waiters_to_requeue.len() - i,
        {
            waiters_to_requeue[i].reset_futex(requeue_futex.clone());
            i += 1;
        }
        let mut j: usize = 0;
        while j < waiters_to_wake.len()
            invariant
                j <= waiters_to_wake.len(),
            decreases waiters_to_wake.len() - j,
        {
            waiters_to_wake[j].wake();
            j += 1;
        }
        Ok(total)
    }
}

// ── Proofs (issue #833 task 5) ────────────────────────────────────────
//
// The three properties are machine-checked twice: directly as the
// `ensures` of the verified cores above (on the real bodies), and here
// as lemmas that derive the issue's phrasing from those contracts.

impl FutexInner {
    /// Property 1: "wake decreases waiter queue length by at most
    /// `wake_count`." Derived from `collect_wake_waiters`' verified
    /// contract (conservation + bound).
    proof fn wake_decreases_by_at_most_wake_count(
        pre: FutexInner,
        post: FutexInner,
        woken: alloc::vec::Vec<Arc<Waiter>>,
        wake_count: usize,
    )
        requires
            woken.len() <= wake_count,
            queue_len(&post) + woken.len() == queue_len(&pre),
        ensures
            queue_len(&post) <= queue_len(&pre),
            queue_len(&pre) - queue_len(&post) <= wake_count,
    {}

    /// Property 2: "requeue preserves total waiter count across both
    /// futexes." Composes the source-core and target-core contracts:
    /// the source loses exactly (woken + requeued); the target gains
    /// exactly the requeued. No waiter is lost or duplicated.
    proof fn requeue_preserves_total(
        source_pre: FutexInner,
        source_post: FutexInner,
        target_pre: FutexInner,
        target_post: FutexInner,
        wake_count: usize,
        requeue_count: usize,
        wake_set: alloc::vec::Vec<Arc<Waiter>>,
        requeue_set: alloc::vec::Vec<Arc<Waiter>>,
    )
        requires
            // requeue_core's verified contract (source side)
            wake_set.len() <= wake_count,
            requeue_set.len() <= requeue_count,
            queue_len(&source_post) + wake_set.len() + requeue_set.len()
                == queue_len(&source_pre),
            // insert_requeued's verified contract (target side)
            queue_len(&target_post) == queue_len(&target_pre) + requeue_set.len(),
        ensures
            queue_len(&source_post) + queue_len(&target_post) + wake_set.len()
                == queue_len(&source_pre) + queue_len(&target_pre),
    {}

    /// Property 3: "ownership is `None` after `wake`." The verified
    /// core is stronger than the issue's phrasing: the owner is
    /// cleared for ANY wake_count (including zero), matching the
    /// Zircon doc ("set to nothing, regardless of the wake count").
    proof fn wake_clears_owner(post: FutexInner)
        requires
            post.owner is None,
        ensures
            post.owner is None,
    {}
}

} // verus!
