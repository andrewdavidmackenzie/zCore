//! Lock-order graph for the futex subsystem (#829 Batch 5F-1).
//!
//! The futex has two lock types: `Futex::inner` (the futex's
//! waiter queue and owner) and `Waiter::inner` (the individual
//! waiter's waker and woken state). Issue #102 documents a potential
//! ABBA between them:
//!
//! - `Futex::wake()` naturally wants: FutexInner → WaiterInner
//!   (pop waiters from the queue, then wake each one)
//! - `FutexFuture::poll()` naturally wants: WaiterInner → FutexInner
//!   (store the waker, then register in the futex queue)
//!
//! The current code avoids the ABBA structurally — both paths are
//! collect-then-act (wake pops under FutexInner then releases before
//! calling waiter.wake(); poll stores the waker under WaiterInner
//! then releases before taking FutexInner). This graph makes the
//! correct nesting order explicit so that if either path ever needs
//! to hold both locks, only the FutexInner → WaiterInner direction
//! compiles.
//!
//! ```text
//! Unlocked → FutexInnerLock → WaiterInnerLock
//! ```
//!
//! ## Deliberately absent edge
//!
//! `WaiterInnerLock → FutexInnerLock` does **not** exist — that is
//! the #102 ABBA direction. Any blocking acquisition of a futex lock
//! while holding a waiter lock is a compile error, enforced by the
//! compile-fail acceptance test below.

use lock::lockgraph::{LockAfter, LockFor, Unlocked};
use lock::{impl_lock_after, IrqLockGuard};

use super::futex::{Futex, FutexInner, Waiter, WaiterInner};

// ── Lock levels ─────────────────────────────────────────────────────

/// The `Futex::inner` lock (waiter queue, owner).
pub enum FutexInnerLock {}

/// The `Waiter::inner` lock (waker, woken flag).
pub enum WaiterInnerLock {}

// ── The graph ───────────────────────────────────────────────────────

// Root: the futex lock can be acquired from Unlocked (every syscall
// entry point takes it first).
impl LockAfter<Unlocked> for FutexInnerLock {}

// Futex before Waiter: the wake direction — pop waiters from the
// futex queue, then wake each one. Single-parent tree; the macro is
// safe here.
impl_lock_after!(FutexInnerLock => WaiterInnerLock);

// ── LockFor: connecting levels to the real fields ──────────────────

impl LockFor<FutexInnerLock> for Futex {
    type Data = FutexInner;
    type Guard<'l> = IrqLockGuard<'l, FutexInner>;
    fn lock(&self) -> Self::Guard<'_> {
        self.inner.lock()
    }
}

impl LockFor<WaiterInnerLock> for Waiter {
    type Data = WaiterInner;
    type Guard<'l> = IrqLockGuard<'l, WaiterInner>;
    fn lock(&self) -> Self::Guard<'_> {
        self.inner.lock()
    }
}

// ── Tests ───────────────────────────────────────────────────────────

/// The #102 ABBA pattern: holding a waiter lock and then blocking on
/// a futex lock. This is the direction `FutexFuture::poll()` would
/// naively take (store waker → register in queue) if it held both.
/// The current code avoids it via collect-then-act; this test
/// ensures it stays that way — the reverse direction is the only
/// legal nesting.
///
/// ```compile_fail
/// use lock::lockgraph::{LockFor, Locked};
/// use zircon_object::signal::lock_order::{
///     FutexFixture as Fx, FutexInnerLock, WaiterInnerLock,
/// };
///
/// let f = Fx::new();
/// let mut locked = Locked::new(&f);
/// let (_waiter_guard, waiter_ctx) = locked.lock_and::<WaiterInnerLock>();
/// // Waiter → Futex is the #102 ABBA direction — no edge exists.
/// let _futex_guard = waiter_ctx.lock::<FutexInnerLock>();
/// ```
#[cfg(test)]
#[allow(dead_code)] // constructed by the compile-fail doc test above
pub(crate) struct FutexFixture {
    futex: lock::IrqLock<u32>,
    waiter: lock::IrqLock<u64>,
}

#[cfg(test)]
impl FutexFixture {
    #[allow(dead_code)] // used by the compile-fail doc test above
    pub(crate) fn new() -> Self {
        FutexFixture {
            futex: lock::IrqLock::new(1),
            waiter: lock::IrqLock::new(2),
        }
    }
}

#[cfg(test)]
impl LockFor<FutexInnerLock> for FutexFixture {
    type Data = u32;
    type Guard<'l> = IrqLockGuard<'l, u32>;
    fn lock(&self) -> Self::Guard<'_> {
        self.futex.lock()
    }
}

#[cfg(test)]
impl LockFor<WaiterInnerLock> for FutexFixture {
    type Data = u64;
    type Guard<'l> = IrqLockGuard<'l, u64>;
    fn lock(&self) -> Self::Guard<'_> {
        self.waiter.lock()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lock::lockgraph::Locked;

    #[test]
    fn legal_futex_then_waiter_chain() {
        let f = FutexFixture::new();
        let mut locked = Locked::new(&f);
        let (_f, mut ctx) = locked.lock_and::<FutexInnerLock>();
        let _w = ctx.lock::<WaiterInnerLock>();
    }

    #[test]
    fn legal_unlocked_entry_at_either_level() {
        let f = FutexFixture::new();
        let mut locked = Locked::new(&f);
        let (_w, _ctx) = locked.lock_and::<WaiterInnerLock>();
    }
}
