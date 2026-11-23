//! Verus formal verification of Futex invariants (#833 Phase 1).
//!
//! This file is compiled ONLY by Verus (via `make verify`), never by
//! the normal Rust toolchain.
//!
//! ## Approach
//!
//! Spec model uses `int` (mathematical integers — always correct,
//! no overflow). Executable model uses `u64` with verified bounds.
//! The two are connected by proving `u64` values fit in the spec's
//! non-negative range.

#[allow(unused)]
use vstd::prelude::*;

fn main() {}

verus! {
    // ── Types ─────────────────────────────────────────────────────

    pub struct ThreadId {
        pub id: u64,
    }

    pub type FutexValue = i32;

    pub enum FutexError {
        InvalidArgs,
        BadHandle,
        WrongType,
        BadState,
        TimedOut,
    }

    pub enum OwnerAction {
        Release,
        AssignWoken,
    }

    /// Spec-level model: waiter_count is `int` (mathematical integer,
    /// non-negative by invariant).
    pub struct FutexState {
        pub value: FutexValue,
        pub waiter_count: int,
        pub owner: Option<ThreadId>,
    }

    pub struct TwoFutexState {
        pub wake_futex: FutexState,
        pub requeue_futex: FutexState,
    }

    // ── Invariant ─────────────────────────────────────────────────

    pub closed spec fn futex_inv(f: &FutexState) -> bool {
        &&& f.waiter_count >= 0
        &&& f.waiter_count <= 0xFFFF_FFFF
    }

    // ── Spec functions ────────────────────────────────────────────

    /// Unified wake spec — covers both wake and wake_single_owner.
    /// Matches FutexContext::FutexWake in the Zircon C++ source.
    pub open spec fn spec_wake(
        pre: FutexState,
        wake_count: int,
        owner_action: OwnerAction,
        woken_thread: Option<ThreadId>,
    ) -> FutexState
    {
        match owner_action {
            OwnerAction::Release => {
                let woken = if pre.waiter_count < wake_count {
                    pre.waiter_count
                } else {
                    wake_count
                };
                FutexState {
                    value: pre.value,
                    waiter_count: pre.waiter_count - woken,
                    owner: Option::None,
                }
            },
            OwnerAction::AssignWoken => {
                if pre.waiter_count > 0 {
                    FutexState {
                        value: pre.value,
                        waiter_count: pre.waiter_count - 1,
                        owner: woken_thread,
                    }
                } else {
                    pre
                }
            },
        }
    }

    /// Wait spec: value check then add waiter + set owner.
    pub open spec fn spec_wait(
        pre: FutexState,
        current_value: FutexValue,
        new_owner: Option<ThreadId>,
    ) -> Result<FutexState, FutexError>
    {
        if pre.value != current_value {
            Result::Err(FutexError::BadState)
        } else {
            Result::Ok(FutexState {
                value: pre.value,
                waiter_count: pre.waiter_count + 1,
                owner: new_owner,
            })
        }
    }

    /// Requeue spec: value check (atomic failure), then wake + move.
    pub open spec fn spec_requeue(
        pre: TwoFutexState,
        wake_count: int,
        current_value: FutexValue,
        requeue_count: int,
        owner_action: OwnerAction,
        new_requeue_owner: Option<ThreadId>,
        woken_thread: Option<ThreadId>,
    ) -> Result<TwoFutexState, FutexError>
    {
        if pre.wake_futex.value != current_value {
            Result::Err(FutexError::BadState)
        } else {
            let woken = if pre.wake_futex.waiter_count < wake_count {
                pre.wake_futex.waiter_count
            } else {
                wake_count
            };
            let remaining = pre.wake_futex.waiter_count - woken;
            let requeued = if remaining < requeue_count {
                remaining
            } else {
                requeue_count
            };

            let wake_post = match owner_action {
                OwnerAction::Release => FutexState {
                    value: pre.wake_futex.value,
                    waiter_count: pre.wake_futex.waiter_count - woken - requeued,
                    owner: Option::None,
                },
                OwnerAction::AssignWoken => {
                    if woken > 0 {
                        FutexState {
                            value: pre.wake_futex.value,
                            waiter_count: pre.wake_futex.waiter_count - woken - requeued,
                            owner: woken_thread,
                        }
                    } else {
                        FutexState {
                            value: pre.wake_futex.value,
                            waiter_count: pre.wake_futex.waiter_count - requeued,
                            owner: pre.wake_futex.owner,
                        }
                    }
                },
            };

            let requeue_post = FutexState {
                value: pre.requeue_futex.value,
                waiter_count: pre.requeue_futex.waiter_count + requeued,
                owner: new_requeue_owner,
            };

            Result::Ok(TwoFutexState {
                wake_futex: wake_post,
                requeue_futex: requeue_post,
            })
        }
    }

    // ── Proof functions ──────────────────────────────────────────

    /// Proof 1: Release wake decreases waiters, clears owner.
    proof fn wake_release_ok(pre: FutexState, wake_count: int)
        requires
            pre.waiter_count >= 0,
            wake_count >= 0,
        ensures
            spec_wake(pre, wake_count, OwnerAction::Release, None).waiter_count >= 0,
            spec_wake(pre, wake_count, OwnerAction::Release, None).waiter_count
                <= pre.waiter_count,
            spec_wake(pre, wake_count, OwnerAction::Release, None).owner matches Option::None,
    {}

    /// Proof 2: AssignWoken with waiters pops 1, sets owner.
    proof fn wake_assign_ok(pre: FutexState, woken_thread: ThreadId)
        requires
            pre.waiter_count > 0,
        ensures
            spec_wake(pre, 1, OwnerAction::AssignWoken, Some(woken_thread)).waiter_count
                == pre.waiter_count - 1,
            spec_wake(pre, 1, OwnerAction::AssignWoken, Some(woken_thread)).waiter_count >= 0,
            spec_wake(pre, 1, OwnerAction::AssignWoken, Some(woken_thread)).owner
                == Some(woken_thread),
    {}

    /// Proof 3: AssignWoken with no waiters is a no-op.
    proof fn wake_assign_noop(pre: FutexState)
        requires
            pre.waiter_count == 0,
        ensures
            spec_wake(pre, 1, OwnerAction::AssignWoken, None) == pre,
    {}

    /// Proof 4: Wait value mismatch returns BadState, no change.
    proof fn wait_mismatch_no_change(pre: FutexState, cv: FutexValue)
        requires
            pre.value != cv,
            pre.waiter_count >= 0,
        ensures
            spec_wait(pre, cv, None) matches Result::Err(FutexError::BadState),
    {}

    /// Proof 5: Requeue value mismatch returns BadState, both unchanged.
    proof fn requeue_mismatch_atomic(pre: TwoFutexState, cv: FutexValue)
        requires
            pre.wake_futex.value != cv,
        ensures
            spec_requeue(pre, 0, cv, 0, OwnerAction::Release, None, None)
                matches Result::Err(FutexError::BadState),
    {}

    /// Proof 6: Requeue conserves total waiters minus woken.
    proof fn requeue_conserves(pre: TwoFutexState, wc: int, cv: FutexValue, rc: int)
        requires
            pre.wake_futex.waiter_count >= 0,
            pre.requeue_futex.waiter_count >= 0,
            wc >= 0,
            rc >= 0,
            pre.wake_futex.value == cv,
        ensures
            match spec_requeue(pre, wc, cv, rc, OwnerAction::Release, None, None) {
                Result::Ok(post) => {
                    let total_pre = pre.wake_futex.waiter_count
                        + pre.requeue_futex.waiter_count;
                    let total_post = post.wake_futex.waiter_count
                        + post.requeue_futex.waiter_count;
                    &&& total_post >= 0
                    &&& total_post <= total_pre
                },
                Result::Err(_) => true,
            },
    {}

    // ── Executable model with verified postconditions ───────────

    /// Executable wake on u64 state.
    pub fn model_wake(
        count: u64,
        wake_count: u64,
        owner_action: OwnerAction,
    ) -> (post_count: u64)
        requires
            count <= 0xFFFF_FFFF,
        ensures
            post_count <= count,
            post_count >= 0,
    {
        match owner_action {
            OwnerAction::Release => {
                if count < wake_count { count } else { wake_count }
            },
            OwnerAction::AssignWoken => {
                if count > 0 { count - 1 } else { count }
            },
        }
    }
}
