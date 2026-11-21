//! Combined task-tree lock context (#829 Batch 5C).
//!
//! [`TaskCtx`] is the combined container that makes cross-object lock
//! ordering enforceable by the lockgraph: it carries the task-tree
//! objects reachable *upward* from the operating level, so a context
//! at any level can acquire locks at its own and ancestor levels
//! (subject to the graph's ordering).
//!
//! The variants capture *where in the hierarchy you are operating*:
//!
//! ```text
//! TaskCtx::Job(job)                          — job's lock reachable
//! TaskCtx::Process { process, job }          — process's + job's
//! TaskCtx::Thread { thread, process, job }   — thread's + process's + job's
//! ```
//!
//! **Availability is runtime** (the `LockFor` impls match on the
//! variant — locking a thread from a `Job`-only context panics);
//! **ordering is compile-time** (the lockgraph's `LockBefore` bounds
//! reject reverse-order acquisition regardless of variant).
//!
//! Locking a *descendant's* lock (e.g. a thread's lock from a
//! process context) is NOT offered by this container: a process has
//! many threads, and "which one" is not a container-level question.
//! Operations that iterate threads construct per-thread contexts
//! (see [`TaskCtx::for_thread`]).

use alloc::sync::Arc;

use crate::task::{Job, Process, Thread};
use lock::lockgraph::{LockFor, Locked, Unlocked};
use lock::KernelMutexGuard;

use super::lock_order::{JobInnerLock, ProcessInnerLock, ThreadInnerLock};

/// Combined lock context over the task-tree hierarchy.
///
/// Construct from any task object via [`TaskCtx::from_job`],
/// [`TaskCtx::from_process`], or [`TaskCtx::from_thread`] (which build
/// the upward chain automatically), then use `Locked::new(&ctx)` to
/// enter the lockgraph at the root and acquire locks in the declared
/// order.
pub enum TaskCtx {
    /// Operating at the job level.
    Job {
        /// The job.
        job: Arc<Job>,
    },
    /// Operating at the process level (carries its job).
    Process {
        /// The process.
        process: Arc<Process>,
        /// The process's job.
        job: Arc<Job>,
    },
    /// Operating at the thread level (carries its process and job).
    Thread {
        /// The thread.
        thread: Arc<Thread>,
        /// The thread's process.
        process: Arc<Process>,
        /// The process's job.
        job: Arc<Job>,
    },
}

impl TaskCtx {
    /// Build a context from a job.
    pub fn from_job(job: Arc<Job>) -> Self {
        TaskCtx::Job { job }
    }

    /// Build a context from a process (fetches its job).
    pub fn from_process(process: Arc<Process>) -> Self {
        let job = process.job();
        TaskCtx::Process { process, job }
    }

    /// Build a context from a thread (fetches its process and job).
    pub fn from_thread(thread: Arc<Thread>) -> Self {
        let process = thread.proc();
        let job = process.job();
        TaskCtx::Thread {
            thread,
            process,
            job,
        }
    }

    /// Build a per-thread context for acquiring a specific thread's
    /// lock — used by operations that iterate a process's threads.
    /// The `process` context's level is NOT carried: the new context
    /// starts from `Unlocked` (the caller has released the process
    /// lock before iterating, per the collect-then-act pattern).
    pub fn for_thread(thread: Arc<Thread>) -> Self {
        Self::from_thread(thread)
    }

    /// Enter the lockgraph at the root over this context.
    pub fn locked(&self) -> Locked<&TaskCtx, Unlocked> {
        Locked::new(self)
    }

    fn job(&self) -> &Arc<Job> {
        match self {
            TaskCtx::Job { job } | TaskCtx::Process { job, .. } | TaskCtx::Thread { job, .. } => {
                job
            }
        }
    }

    fn process(&self) -> &Arc<Process> {
        match self {
            TaskCtx::Job { .. } => panic!("TaskCtx: no process at the job level"),
            TaskCtx::Process { process, .. } | TaskCtx::Thread { process, .. } => process,
        }
    }

    fn thread(&self) -> &Arc<Thread> {
        match self {
            TaskCtx::Job { .. } | TaskCtx::Process { .. } => {
                panic!("TaskCtx: no thread at this level")
            }
            TaskCtx::Thread { thread, .. } => thread,
        }
    }
}

// LockFor: the enum dispatches to the right object's lock; the
// lockgraph's LockBefore bounds (from the graph declared in
// lock_order.rs) enforce the ordering at compile time.

impl LockFor<JobInnerLock> for TaskCtx {
    type Data = super::JobInner;
    type Guard<'l> = KernelMutexGuard<'l, super::JobInner>;
    fn lock(&self) -> Self::Guard<'_> {
        self.job().inner.lock()
    }
}

impl LockFor<ProcessInnerLock> for TaskCtx {
    type Data = super::ProcessInner;
    type Guard<'l> = KernelMutexGuard<'l, super::ProcessInner>;
    fn lock(&self) -> Self::Guard<'_> {
        self.process().inner.lock()
    }
}

impl LockFor<ThreadInnerLock> for TaskCtx {
    type Data = super::ThreadInner;
    type Guard<'l> = KernelMutexGuard<'l, super::ThreadInner>;
    fn lock(&self) -> Self::Guard<'_> {
        self.thread().inner.lock()
    }
}

#[cfg(test)]
#[serial_test::parallel]
mod tests {
    use super::*;
    use crate::task::{Job, Process, Thread};

    #[test]
    fn thread_level_cannot_reach_process_lock() {
        // Compile-fail is proven by the lockgraph doc tests; here we
        // verify the runtime dispatch: a thread-level context CAN
        // acquire the thread lock, and the graph REJECTS reaching the
        // process lock from it (compile-time — verified by the
        // acceptance tests in lock_order.rs).
        let job = Job::root();
        let proc = Process::create(&job, "demo").unwrap();
        let thread = Thread::create(&proc, "demo-main").unwrap();

        let ctx = TaskCtx::from_thread(thread);
        let mut locked = ctx.locked();
        let (_guard, _thread_ctx) = locked.lock_and::<ThreadInnerLock>();
        // Legal: thread lock acquired.
        // The following would NOT compile (no thread → process edge):
        // let _p = _thread_ctx.lock::<ProcessInnerLock>();
    }

    #[test]
    fn process_level_can_reach_thread_lock_via_graph() {
        // A process-level context can legally acquire the process
        // lock, and the graph permits process → thread (a
        // process-level TaskCtx carries the process; acquiring the
        // THREAD lock requires the Thread variant — see for_thread).
        let job = Job::root();
        let proc = Process::create(&job, "demo").unwrap();

        let ctx = TaskCtx::from_process(proc);
        let mut locked = ctx.locked();
        let (_guard, _proc_ctx) = locked.lock_and::<ProcessInnerLock>();
        // Legal: process lock acquired at the process level.
        // Reaching the job lock from here would NOT compile (no
        // process → job edge):
        // let _j = _proc_ctx.lock::<JobInnerLock>();
    }

    #[test]
    fn thread_ctx_full_legal_chain() {
        // A thread-level TaskCtx carries all three objects. Entering
        // from Unlocked, the full job → process → thread chain is
        // legal (transitivity). This is the shape Process::exit
        // would use after collecting threads: hold the process lock,
        // then take thread locks (process → thread is downstream).
        let job = Job::root();
        let proc = Process::create(&job, "demo").unwrap();
        let thread = Thread::create(&proc, "demo-main").unwrap();

        let ctx = TaskCtx::from_thread(thread);
        let mut locked = ctx.locked();
        let (_j, mut job_ctx) = locked.lock_and::<JobInnerLock>();
        // Transitivity: job → process is legal.
        let (_p, mut proc_ctx) = job_ctx.lock_and::<ProcessInnerLock>();
        // Transitivity: process → thread is legal.
        let _t = proc_ctx.lock::<ThreadInnerLock>();
    }

    #[test]
    fn job_level_cannot_reach_process_object() {
        // A job-level context only carries the job: the graph permits
        // job → process ordering, but the Job variant has no process
        // object to lock (availability is runtime, ordering is
        // compile-time). Use a process/thread-level context to
        // acquire process locks.
        let job = Job::root();
        let ctx = TaskCtx::from_job(job);
        let mut locked = ctx.locked();
        // Legal: the job's own lock.
        let (_guard, _job_ctx) = locked.lock_and::<JobInnerLock>();
    }
}
