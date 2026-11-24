//! Kernel-object invariants (#888, from #833).
//!
//! Invariants are declared once, as plain Rust, and enforced where
//! each rule admits it:
//!
//! 1. **Inline asserts** — [`inv!`] at the mutation sites that must
//!    preserve the rule. Compiled only when the `invariant-checks`
//!    feature is enabled; test kernels (`rust-tests` via
//!    `baremetal-test`) and the host test builds enable it, production
//!    kernels do not.
//! 2. **Sweeps** — `check_invariants()`-style walks invoked from tests
//!    after operations, for whole-object-graph properties.
//!
//! Structural rules that can be encoded in types are enforced at
//! compile time instead (the lockgraph, #829) and are not listed here.
//! Concurrency/interleaving properties are covered by the race and
//! stress test suites, not by inline asserts.
//!
//! ## Registry
//!
//! ### Futex (#888 Batch 1)
//!
//! | ID | Invariant | Source | Enforcement |
//! |---|---|---|---|
//! | INV-FUTEX-1 | `wake(n)` returns `r` with `r ≤ n` and `r ≤ queued waiters` | [zx_futex_wake](https://fuchsia.dev/fuchsia-src/reference/syscalls/futex_wake) | inline in `Futex::wake` |
//! | INV-FUTEX-2 | Departing waiters never consume a wake-count slot: the wake loop runs until the budget is spent or the queue is exhausted (race-free form — exact non-departing counts race with concurrent drops) | #862/#875 (Linux skips `FUTEX_STATE_EXITING` likewise) | inline in `Futex::wake` |
//! | INV-FUTEX-3 | After `wake` (any count), the futex has no owner | [zx_futex_wake](https://fuchsia.dev/fuchsia-src/reference/syscalls/futex_wake) | inline in `Futex::wake` |
//! | INV-FUTEX-4 | `wake_single_owner`: the owner is the woken waiter's thread, or none if no waiter/thread | [zx_futex_wake_single_owner](https://fuchsia.dev/fuchsia-src/reference/syscalls/futex_wake_single_owner) | inline in `wake_single_owner` |
//! | INV-FUTEX-5 | Requeue conserves waiters: `woken + requeued + discarded-departing + remaining == initial` (departing waiters are popped and discarded by the requeue loop) | [zx_futex_requeue](https://fuchsia.dev/fuchsia-src/reference/syscalls/futex_requeue) | inline in `requeue` (source side, under both locks) |
//! | INV-FUTEX-6 | Requeue failure is atomic: on `Err`, neither futex's queue or owner changed | [zx_futex_requeue](https://fuchsia.dev/fuchsia-src/reference/syscalls/futex_requeue) | inline in `requeue` error paths |
//! | INV-FUTEX-7 | The owner of a futex is never simultaneously a waiter on it | [futex docs](https://fuchsia.dev/fuchsia-src/reference/kernel_objects/futex) | TBD (cross-object sweep candidate) |
//! | INV-FUTEX-8 | `wait` value mismatch returns `BAD_STATE` without queueing | [futex docs](https://fuchsia.dev/fuchsia-src/reference/kernel_objects/futex) | Phase 2 (async path) |
//!
//! The target-side requeue conservation is deliberately NOT asserted
//! across the lock release: between dropping the futex locks and
//! re-acquiring the target lock, other threads may legitimately mutate
//! the target queue, so a post-insertion length comparison could
//! false-positive. The source side is asserted under continuously held
//! locks.
//!
//! ### Process (#888 Batch 1)
//!
//! | ID | Invariant | Source | Enforcement |
//! |---|---|---|---|
//! | INV-PROCESS-1 | Thread-list ↔ back-ref consistency: every live thread `t` in `p`'s list has `t.proc() == p` | [process docs](https://fuchsia.dev/fuchsia-src/reference/kernel_objects/process) | inline in `add_thread` + `check_thread_invariants` sweep |
//! | INV-PROCESS-2 | Status monotonicity: `Init → Running → Exited`, never backwards; `Exited` written at most once | [process docs](https://fuchsia.dev/fuchsia-src/reference/kernel_objects/process) | inline at status writes |
//! | INV-PROCESS-3 | Handle values are unique in the process handle table | [process docs](https://fuchsia.dev/fuchsia-src/reference/kernel_objects/process) | inline in `add_handle` |
//! | INV-PROCESS-4 | Handle-count pairing, race-free form: an object with a handle in a table has a positive count; a decrement never runs on zero (exact before/after snapshots race with other processes — review finding on #891) | our `KObjectBase` inc/dec pairing | inline in add/remove handle |
//! | INV-PROCESS-5 | Futex-table keying: every entry `(addr, futex)` has `futex.user_addr == addr` | our `ProcessInner::futexes` | inline in `get_futex` |
//! | INV-PROCESS-6 | Last-thread rule: when the thread list empties, the process is `Exited` | [process docs](https://fuchsia.dev/fuchsia-src/reference/kernel_objects/process) | inline in `remove_thread` |
//!
//! ### Batch 2 candidates (see #888)
//!
//! Job (INV-JOB-1..3), Channel (INV-CHANNEL-1..4), Port (INV-PORT-1..2),
//! Socket (INV-SOCKET-1..2), VmObject (INV-VMO-1..3).

/// Assert a kernel-object invariant.
///
/// Compiled to nothing unless the `invariant-checks` feature is
/// enabled (test kernels enable it). The asserted condition is only
/// type-checked in feature-enabled builds, like `#[cfg(test)]` code.
///
/// ```
/// # use zircon_object::inv;
/// let x = 1;
/// inv!(x == 1, "INV-EXAMPLE-1: x must be 1, got {}", x);
/// inv!(x > 0);
/// ```
#[macro_export]
macro_rules! inv {
    ($cond:expr) => {
        #[cfg(feature = "invariant-checks")]
        {
            assert!($cond, "invariant violated: {}", stringify!($cond));
        }
    };
    ($cond:expr, $($arg:tt)+) => {
        #[cfg(feature = "invariant-checks")]
        {
            assert!($cond, $($arg)+);
        }
    };
}

// Re-export so `#[macro_use] mod invariants;` in lib.rs makes the
// macro available crate-wide under both spellings.
pub use inv;
