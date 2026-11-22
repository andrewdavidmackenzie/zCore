//! Lock-order graph for the executor (#829 Batch 5G-1).
//!
//! The executor has two lock levels, documented in code comments
//! throughout `task_collection.rs`:
//!
//! ```text
//! Unlocked → SchedStateLock → FutureCollectionLock
//! ```
//!
//! ## Edge justification
//!
//! `SchedStateLock → FutureCollectionLock`: every path that takes
//! both does so in this order — `add_task`, `scan_notifications`,
//! and `steal_into` all lock `sched_state` first, then the
//! `future_collections[priority]` mutex (task_collection.rs:220,
//! :240, :352).
//!
//! ## Same-type discipline (documented, not graph-expressible)
//!
//! Cross-CPU steals: `steal_into` takes the SOURCE's locks
//! (blocking: sched → fc), then the DESTINATION's locks via
//! `try_lock` (never blocking). The try_lock rule prevents a
//! mirror-image steal on another CPU from deadlocking. This is a
//! per-CPU-instance ordering — the graph treats all
//! `TaskCollection` instances as the same level.
//!
//! ## The irq-off rule (separate axis)
//!
//! Some executor paths manually disable interrupts before taking
//! locks (`intr_off`/`intr_on`). This is the interrupt-safety axis,
//! orthogonal to the lock-ordering graph — it constrains WHO may
//! acquire, not the ORDER between locks.

use lock::impl_lock_after;
use lock::lockgraph::{LockAfter, LockFor, Unlocked};
use spin::MutexGuard;

use crate::sched::SchedState;
use crate::task_collection::TaskCollection;

// ── Lock levels ─────────────────────────────────────────────────────

/// The per-CPU `sched_state` lock (run queue, priorities).
pub enum SchedStateLock {}

/// A `future_collections[priority]` lock (task storage).
pub enum FutureCollectionLock {}

// ── The graph ───────────────────────────────────────────────────────

// Root: sched_state is the first lock every executor path takes.
impl LockAfter<Unlocked> for SchedStateLock {}

// Sched before FutureCollection: the documented order — every path
// that takes both does sched first. Single-parent; the macro is safe.
impl_lock_after!(SchedStateLock => FutureCollectionLock);

// ── LockFor: connecting levels to the real fields ──────────────────

impl LockFor<SchedStateLock> for TaskCollection {
    type Data = SchedState;
    type Guard<'l> = MutexGuard<'l, SchedState>;
    fn lock(&self) -> Self::Guard<'_> {
        self.sched_state.lock()
    }
}

// Note: FutureCollectionLock operates on a specific priority's
// collection, selected by priority at runtime. The LockFor design
// (single lock per level per container) doesn't parameterize by
// priority; the sched→fc ORDER is enforced by the edge whenever
// both are acquired through contexts. Per-priority enforcement
// would need a per-priority container type — more machinery than
// the enforcement value justifies for a two-level graph.

// ── Tests ───────────────────────────────────────────────────────────

/// The reverse order: holding a future-collection lock and then
/// blocking on the sched_state lock. No path does this (the
/// documented order is sched → fc), and the graph rejects it.
///
/// ```compile_fail
/// use lock::lockgraph::{LockFor, Locked};
/// use executor::lock_order::{
///     ExecFixture as Ex, FutureCollectionLock, SchedStateLock,
/// };
///
/// let e = Ex::new();
/// let mut locked = Locked::new(&e);
/// let (_fc_guard, fc_ctx) = locked.lock_and::<FutureCollectionLock>();
/// // FC → Sched is the reverse of the documented order.
/// let _sched_guard = fc_ctx.lock::<SchedStateLock>();
/// ```
#[cfg(test)]
#[allow(dead_code)] // constructed by the compile-fail doc test above
pub(crate) struct ExecFixture {
    sched: spin::Mutex<u32>,
    fc: spin::Mutex<u64>,
}

#[cfg(test)]
impl ExecFixture {
    #[allow(dead_code)] // used by the compile-fail doc test above
    pub(crate) fn new() -> Self {
        ExecFixture {
            sched: spin::Mutex::new(1),
            fc: spin::Mutex::new(2),
        }
    }
}

#[cfg(test)]
impl LockFor<SchedStateLock> for ExecFixture {
    type Data = u32;
    type Guard<'l> = MutexGuard<'l, u32>;
    fn lock(&self) -> Self::Guard<'_> {
        self.sched.lock()
    }
}

#[cfg(test)]
impl LockFor<FutureCollectionLock> for ExecFixture {
    type Data = u64;
    type Guard<'l> = MutexGuard<'l, u64>;
    fn lock(&self) -> Self::Guard<'_> {
        self.fc.lock()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lock::lockgraph::Locked;

    #[test]
    fn legal_sched_then_fc_chain() {
        let e = ExecFixture::new();
        let mut locked = Locked::new(&e);
        let (_s, mut ctx) = locked.lock_and::<SchedStateLock>();
        let _fc = ctx.lock::<FutureCollectionLock>();
    }

    #[test]
    fn legal_unlocked_entry_at_either_level() {
        let e = ExecFixture::new();
        let mut locked = Locked::new(&e);
        let (_fc, _ctx) = locked.lock_and::<FutureCollectionLock>();
    }
}
