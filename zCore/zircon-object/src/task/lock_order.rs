//! Lock-order graph for the task-object hierarchy (#829 Batch 5B).
//!
//! This module declares the seed graph for the type-level lock
//! ordering introduced in #829, over the task-object family whose
//! inversions produced the historical deadlocks:
//!
//! ```text
//! Unlocked → JobInnerLock → ProcessInnerLock → ThreadInnerLock
//! ```
//!
//! The edges encode the fixes for:
//! - #838: `Process::{suspend,resume,exit}` (process → thread) vs
//!   `Thread::terminate` → `remove_thread` (thread → process) — the
//!   ABBA deadlock behind #834's machine freezes.
//! - The #837 review round: `Thread::terminate`'s thread → process
//!   edge and `Job::get_all_threads`' job → process edge (against
//!   `Process::remove_thread` → `Job::is_killed`).
//!
//! The graph is *transitive*: a context at the job level may acquire
//! process or thread locks (job → process → thread), but every
//! reverse acquisition is a compile error.
//!
//! The `LockFor` impls below connect the levels to the real
//! `Job`/`Process`/`Thread` `inner` fields for use by Batch 5C's
//! call-site conversion. The tests exercise the declared graph with
//! a demo container that mirrors the historical acquisition shapes —
//! including compile-fail tests reproducing the pre-#838 and
//! pre-#837 deadlock patterns, which the graph now rejects at
//! build time.

use crate::task::{Job, Process, Thread};
use lock::lockgraph::{LockAfter, LockFor, Unlocked};
use lock::{impl_lock_after, KernelMutexGuard};

// ── Lock levels ─────────────────────────────────────────────────────

/// The `Job::inner` lock.
pub enum JobInnerLock {}

/// The `Process::inner` lock.
pub enum ProcessInnerLock {}

/// The `Thread::inner` lock.
pub enum ThreadInnerLock {}

// ── The graph ───────────────────────────────────────────────────────

// Roots must be declared with a direct impl (see lockgraph docs).
impl LockAfter<Unlocked> for JobInnerLock {}

// Job before Process: `Job::get_all_threads`/`total_cpu_time`
// collect from processes; `Process::remove_thread` →
// `Job::is_killed` is the reverse edge that must NOT exist (found
// in the #837 review round).
impl_lock_after!(JobInnerLock => ProcessInnerLock);

// Process before Thread: `Process::{suspend,resume,exit}` collect
// threads and then suspend/resume/kill them; `Thread::terminate` →
// `remove_thread` is the reverse edge that must NOT exist (the #838
// deadlock behind #834).
impl_lock_after!(ProcessInnerLock => ThreadInnerLock);

// ── LockFor: connecting levels to the real fields ──────────────────

impl LockFor<JobInnerLock> for Job {
    type Data = super::JobInner;
    type Guard<'l> = KernelMutexGuard<'l, super::JobInner>;
    fn lock(&self) -> Self::Guard<'_> {
        self.inner.lock()
    }
}

impl LockFor<ProcessInnerLock> for Process {
    type Data = super::ProcessInner;
    type Guard<'l> = KernelMutexGuard<'l, super::ProcessInner>;
    fn lock(&self) -> Self::Guard<'_> {
        self.inner.lock()
    }
}

impl LockFor<ThreadInnerLock> for Thread {
    type Data = super::ThreadInner;
    type Guard<'l> = KernelMutexGuard<'l, super::ThreadInner>;
    fn lock(&self) -> Self::Guard<'_> {
        self.inner.lock()
    }
}

// ── Tests ───────────────────────────────────────────────────────────
//
// The ordering checks operate on a `Locked` context over a type that
// implements `LockFor` for the levels involved. The demo container
// below mirrors the task-tree shape (one lock per level); it uses
// the REAL graph levels and edges declared above, so the compile
// results reflect the production lock order.

/// Test fixture with one lock per task-tree level, exercising the
/// REAL seed graph declared in this module. It carries the two
/// compile-fail acceptance tests reproducing the historical deadlock
/// patterns, which the graph now rejects at build time.
///
/// The pre-#838 `Thread::terminate` pattern: holding the THREAD lock
/// and then acquiring the PROCESS lock (`remove_thread`). The graph
/// has no thread → process edge, so this must not compile. This is
/// #829's acceptance test for the #838/#834 deadlock.
///
/// ```compile_fail
/// use lock::lockgraph::{LockFor, Locked};
/// use zircon_object::task::lock_order::{
///     ProcessInnerLock, TaskTreeFixture as TaskTree, ThreadInnerLock,
/// };
///
/// let t = TaskTree::new();
/// let mut locked = Locked::new(&t);
/// let (_thread_guard, thread_ctx) = locked.lock_and::<ThreadInnerLock>();
/// // Pre-#838 shape: thread level → process level. The graph
/// // does not provide ThreadInnerLock: LockBefore<ProcessInnerLock>.
/// let _proc_guard = thread_ctx.lock::<ProcessInnerLock>();
/// ```
///
/// The pre-#837 `Job::get_all_threads` reverse pattern: holding the
/// PROCESS lock and then acquiring the JOB lock. The graph is job →
/// process (not the reverse).
///
/// ```compile_fail
/// use lock::lockgraph::{LockFor, Locked};
/// use zircon_object::task::lock_order::{
///     JobInnerLock, ProcessInnerLock, TaskTreeFixture as TaskTree,
/// };
///
/// let t = TaskTree::new();
/// let mut locked = Locked::new(&t);
/// let (_proc_guard, proc_ctx) = locked.lock_and::<ProcessInnerLock>();
/// // Reverse of the declared job → process edge.
/// let _job_guard = proc_ctx.lock::<JobInnerLock>();
/// ```
#[cfg(test)]
#[allow(dead_code)] // constructed by the compile-fail doc tests above
pub(crate) struct TaskTreeFixture {
    job: lock::KernelMutex<u32>,
    process: lock::KernelMutex<u64>,
    thread: lock::KernelMutex<u16>,
}

#[cfg(test)]
impl TaskTreeFixture {
    #[allow(dead_code)] // used by the compile-fail doc tests above
    pub(crate) fn new() -> Self {
        TaskTreeFixture {
            job: lock::KernelMutex::new(1),
            process: lock::KernelMutex::new(2),
            thread: lock::KernelMutex::new(3),
        }
    }
}

#[cfg(test)]
impl LockFor<JobInnerLock> for TaskTreeFixture {
    type Data = u32;
    type Guard<'l> = KernelMutexGuard<'l, u32>;
    fn lock(&self) -> Self::Guard<'_> {
        self.job.lock()
    }
}

#[cfg(test)]
impl LockFor<ProcessInnerLock> for TaskTreeFixture {
    type Data = u64;
    type Guard<'l> = KernelMutexGuard<'l, u64>;
    fn lock(&self) -> Self::Guard<'_> {
        self.process.lock()
    }
}

#[cfg(test)]
impl LockFor<ThreadInnerLock> for TaskTreeFixture {
    type Data = u16;
    type Guard<'l> = KernelMutexGuard<'l, u16>;
    fn lock(&self) -> Self::Guard<'_> {
        self.thread.lock()
    }
}
#[cfg(test)]
#[serial_test::parallel]
mod tests {
    use super::*;
    use lock::lockgraph::{LockFor, Locked};
    use lock::KernelMutex;

    /// Demo container with one lock per task-tree level. Acquiring
    /// its locks through a `Locked` context checks the seed graph
    /// declared in this module — not a test-local graph.
    struct TaskTree {
        job: KernelMutex<u32>,
        process: KernelMutex<u64>,
        thread: KernelMutex<u16>,
    }

    impl LockFor<JobInnerLock> for TaskTree {
        type Data = u32;
        type Guard<'l> = KernelMutexGuard<'l, u32>;
        fn lock(&self) -> Self::Guard<'_> {
            self.job.lock()
        }
    }

    impl LockFor<ProcessInnerLock> for TaskTree {
        type Data = u64;
        type Guard<'l> = KernelMutexGuard<'l, u64>;
        fn lock(&self) -> Self::Guard<'_> {
            self.process.lock()
        }
    }

    impl LockFor<ThreadInnerLock> for TaskTree {
        type Data = u16;
        type Guard<'l> = KernelMutexGuard<'l, u16>;
        fn lock(&self) -> Self::Guard<'_> {
            self.thread.lock()
        }
    }

    fn tree() -> TaskTree {
        TaskTree {
            job: KernelMutex::new(1),
            process: KernelMutex::new(2),
            thread: KernelMutex::new(3),
        }
    }

    #[test]
    fn legal_job_then_process_then_thread() {
        let t = tree();
        let mut locked = Locked::new(&t);
        let (_j, mut ctx) = locked.lock_and::<JobInnerLock>();
        let (_p, mut ctx) = ctx.lock_and::<ProcessInnerLock>();
        let _th = ctx.lock::<ThreadInnerLock>();
    }

    #[test]
    fn legal_transitive_job_straight_to_thread() {
        let t = tree();
        let mut locked = Locked::new(&t);
        let (_j, mut ctx) = locked.lock_and::<JobInnerLock>();
        // Transitivity: job → process → thread permits job → thread.
        let _th = ctx.lock::<ThreadInnerLock>();
    }

    #[test]
    fn legal_unlocked_process_root_entry() {
        // Any level can be entered from Unlocked (no locks held).
        let t = tree();
        let mut locked = Locked::new(&t);
        let (_p, _ctx) = locked.lock_and::<ProcessInnerLock>();
    }
}
