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
//! | INV-FUTEX-7 | The owner of a futex is never simultaneously a waiter on it | [futex docs](https://fuchsia.dev/fuchsia-src/reference/kernel_objects/futex) | `check_owner_invariant` sweep (Batch 3) |
//! | INV-FUTEX-8 | `wait` value mismatch returns `BAD_STATE` without queueing | [futex docs](https://fuchsia.dev/fuchsia-src/reference/kernel_objects/futex) | inline in `FutexFuture::poll`'s BAD_STATE branch, under the futex lock (Batch 3) |
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
//! ### Job (#888 Batch 2)
//!
//! | ID | Invariant | Source | Enforcement |
//! |---|---|---|---|
//! | INV-JOB-1 | Tree depth < `MAX_HEIGHT` (32) | Zircon job docs | inline in `create_child` + `check_invariants` sweep |
//! | INV-JOB-2 | The kill code is written exactly once (first writer wins; later kills are no-ops) | job docs + kill propagation | inline in `kill_with_code` |
//! | INV-JOB-3 | `JOB_NO_JOBS` ⟺ children empty; `JOB_NO_PROCESSES` ⟺ processes empty; `JOB_NO_CHILDREN` ⟺ both empty | job docs | `check_invariants` sweep (signal readback outside the job lock — call at quiet points) |
//!
//! ### Channel (#888 Batch 2)
//!
//! | ID | Invariant | Source | Enforcement |
//! |---|---|---|---|
//! | INV-CHANNEL-1 | The receive queue never exceeds `MAX_PENDING_MSG_COUNT` | channel docs | inline in `push_general` (under the queue lock) |
//! | INV-CHANNEL-2 | `READABLE` set ⟺ queue non-empty | channel docs | inline in `push_general` and `check_and_read` (under the queue lock, which also guards the signal update) |
//! | INV-CHANNEL-3 | Closing an endpoint marks the peer `PEER_CLOSED` and clears `WRITABLE` | channel docs | inline in `Drop for Channel` |
//! | INV-CHANNEL-4 | Outstanding call txids are unique | channel_call semantics | inline in `call` (under the call_reply lock) |
//!
//! ### Port (#888 Batch 2)
//!
//! | ID | Invariant | Source | Enforcement |
//! |---|---|---|---|
//! | INV-PORT-1 | Queued packets stay within `MAX_ALLOCATED_PACKET_COUNT_PER_PORT` (4096) — including kernel-sourced packets, which our push path does not itself limit | port docs | inline in `push_user` and `push_with_source` |
//! | INV-PORT-2 | A successful cancel removes the subscription entry entirely (re-framed from "observer uniqueness" — multiple subscriptions per (source, key) are legal in Fuchsia) | wait_async/cancel semantics | inline in `cancel_async` |
//!
//! ### Socket (#888 Batch 2)
//!
//! | ID | Invariant | Source | Enforcement |
//! |---|---|---|---|
//! | INV-SOCKET-1 | Datagram writes are all-or-nothing, and the datagram-length accounting matches the buffered bytes | socket docs | inline in `write_datagram` (under the inner lock) |
//! | INV-SOCKET-2 | Disabling writes clears `WRITABLE`/sets the disable markers; re-enabling restores them | socket disposition model | inline in `set_disposition` |
//!
//! ### VmObject (#888 Batch 2)
//!
//! | ID | Invariant | Source | Enforcement |
//! |---|---|---|---|
//! | INV-VMO-1 | `set_content_size_and_resize` sets `content_size` exactly to the effective new value, and the VMO length covers it | our documented behavior | inline in `set_content_size_and_resize` |
//! | INV-VMO-2 | Commit/decommit ranges stay within the VMO size (zx_vmo_op_range: OUT_OF_RANGE for invalid ranges; Fuchsia's CommitRangeInternal trims) | [zx_vmo_op_range](https://fuchsia.dev/fuchsia-src/reference/syscalls/vmo_op_range) | inline in `commit` and `decommit` (Batch 3) |
//! | INV-VMO-3 | A child slice is wholly contained in the parent | VMO child docs | inline in the slice creation checks |

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
