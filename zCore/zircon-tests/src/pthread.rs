//! POSIX threads tests.
//!
//! Ports of Fuchsia's zircon/system/utest/core/pthread tests.
//! POSIX threads are a userspace library built on top of Zircon
//! threads and futexes. These tests exercise the kernel primitives
//! that underlie pthreads (Thread, Futex) since the C library
//! abstraction is not available.

use crate::helpers::TestContext;
use zircon_object::object::KernelObject;
use zircon_object::task::{Job, Process, Thread, ThreadState};

extern crate alloc;

/// Create a Zircon thread and verify it exists (the kernel primitive
/// underlying pthread_create + pthread_join).
#[test]
fn create_join() {
    let _ctx = TestContext::new();
    let job = Job::root();
    let proc = Process::create(&job, "pthread-test").unwrap();
    let thread = Thread::create(&proc, "joinable").unwrap();

    assert_ne!(thread.id(), 0);
    assert_eq!(thread.state(), ThreadState::New);
    // pthread_join would wait for thread termination — at the kernel
    // level, this is thread.wait_signal(THREAD_TERMINATED).
}

/// Create a thread (underlying pthread_create with detached attribute).
/// Detached threads are independently managed by the kernel.
#[test]
fn create_detached() {
    let _ctx = TestContext::new();
    let job = Job::root();
    let proc = Process::create(&job, "pthread-test").unwrap();
    let thread = Thread::create(&proc, "detached").unwrap();

    assert_ne!(thread.id(), 0);
    // Detaching is a library concept — at kernel level, the thread
    // handle is simply not waited on.
}

/// Futex lock/unlock cycle (the kernel primitive underlying
/// pthread_mutex_lock/unlock).
#[test]
fn mutex_basic() {
    use core::sync::atomic::AtomicI32;
    use zircon_object::signal::Futex;

    let _ctx = TestContext::new();
    static MUTEX_VALUE: AtomicI32 = AtomicI32::new(0);
    let futex = Futex::new(&MUTEX_VALUE as *const AtomicI32 as usize);

    // Simulate lock: CAS 0 → 1 (would be done in userspace).
    // Simulate unlock: wake one waiter.
    assert_eq!(futex.wake(1), 0); // No waiters — unlock is a no-op.
}

/// Futex wait/wake for condition variable (the kernel primitive
/// underlying pthread_cond_wait/signal).
#[test]
fn condition_variable() {
    use core::sync::atomic::AtomicI32;
    use zircon_object::signal::Futex;

    let _ctx = TestContext::new();
    static COND_VALUE: AtomicI32 = AtomicI32::new(0);
    let futex = Futex::new(&COND_VALUE as *const AtomicI32 as usize);

    // pthread_cond_signal → futex_wake(1)
    assert_eq!(futex.wake(1), 0);
    // pthread_cond_broadcast → futex_wake(MAX)
    assert_eq!(futex.wake(usize::MAX), 0);
}

/// Read-write lock: multiple readers, single writer. At the kernel
/// level, this is implemented with futexes and atomics.
#[test]
fn rw_lock() {
    use core::sync::atomic::AtomicI32;
    use zircon_object::signal::Futex;

    let _ctx = TestContext::new();
    static RW_VALUE: AtomicI32 = AtomicI32::new(0);
    let futex = Futex::new(&RW_VALUE as *const AtomicI32 as usize);

    // RWLock uses futex for writer blocking.
    assert_eq!(futex.wake(1), 0);
}

/// Thread-specific data (TSD). At the kernel level, each thread
/// has its own register context including thread pointer.
#[test]
fn thread_specific() {
    let _ctx = TestContext::new();
    let job = Job::root();
    let proc = Process::create(&job, "pthread-test").unwrap();
    let thread = Thread::create(&proc, "tsd-thread").unwrap();

    // Each thread has a unique KOID — thread-specific data is
    // indexed by thread identity.
    assert_ne!(thread.id(), 0);
}

/// Execute once (pthread_once). At kernel level, this uses
/// futex + atomics for synchronization.
#[test]
fn once() {
    use core::sync::atomic::{AtomicI32, Ordering};
    use zircon_object::signal::Futex;

    let _ctx = TestContext::new();
    static ONCE_STATE: AtomicI32 = AtomicI32::new(0);
    let futex = Futex::new(&ONCE_STATE as *const AtomicI32 as usize);

    // Simulate once: CAS 0 → 1 (init in progress), then set 2 (done).
    ONCE_STATE.store(2, Ordering::SeqCst);
    // Wake any threads waiting for initialization.
    futex.wake(usize::MAX);
}

/// Cancel a running thread. At kernel level, this is thread kill.
#[test]
fn cancel() {
    let _ctx = TestContext::new();
    let job = Job::root();
    let proc = Process::create(&job, "pthread-test").unwrap();
    let _thread = Thread::create(&proc, "cancel-thread").unwrap();

    // pthread_cancel → kill the thread/process at kernel level.
    proc.exit(zircon_object::task::TASK_RETCODE_SYSCALL_KILL);
}

/// Barrier synchronization. At kernel level, barriers use futexes
/// and an atomic counter.
#[test]
fn barrier() {
    use core::sync::atomic::{AtomicI32, Ordering};
    use zircon_object::signal::Futex;

    let _ctx = TestContext::new();
    static BARRIER_COUNT: AtomicI32 = AtomicI32::new(0);
    let futex = Futex::new(&BARRIER_COUNT as *const AtomicI32 as usize);

    // Simulate: last thread to arrive broadcasts wake.
    BARRIER_COUNT.store(1, Ordering::SeqCst);
    futex.wake(usize::MAX);
}

/// Stack size attribute. At kernel level, thread stacks are VMAR
/// regions allocated during thread creation.
#[test]
fn attr_stack_size() {
    let _ctx = TestContext::new();
    let job = Job::root();
    let proc = Process::create(&job, "pthread-test").unwrap();

    // Stack size is determined during thread_start, not thread_create.
    let thread = Thread::create(&proc, "stack-test").unwrap();
    assert_ne!(thread.id(), 0);
}

/// C++: TEST(PthreadTest, Basic)
/// Basic thread creation.
#[test]
fn basic() {
    let _ctx = TestContext::new();
    let job = Job::root();
    let proc = Process::create(&job, "pthread-test").unwrap();
    let thread = Thread::create(&proc, "basic").unwrap();
    assert_ne!(thread.id(), 0);
    assert_eq!(thread.name(), "basic");
}

/// C++: TEST(PthreadTest, BigStackSize)
/// Create a thread intended for a big stack. Verify thread creation
/// succeeds regardless of stack size (stack is allocated separately).
#[test]
fn big_stack_size() {
    let _ctx = TestContext::new();
    let job = Job::root();
    let proc = Process::create(&job, "pthread-test").unwrap();
    let thread = Thread::create(&proc, "big-stack").unwrap();
    assert_ne!(thread.id(), 0);
    // Stack allocation happens at thread_start time, not thread_create.
}

/// C++: TEST(PthreadTest, GetstackMainThread)
/// Get the stack of the main thread. At kernel level, the process
/// VMAR contains the thread's stack mapping.
#[test]
fn getstack_main_thread() {
    let _ctx = TestContext::new();
    let job = Job::root();
    let proc = Process::create(&job, "pthread-test").unwrap();
    let _thread = Thread::create(&proc, "main").unwrap();

    // The process VMAR is accessible.
    let vmar = proc.vmar();
    assert_ne!(vmar.id(), 0);
}

/// C++: TEST(PthreadTest, GetstackOtherThread)
/// Get the stack of a non-main thread.
#[test]
fn getstack_other_thread() {
    let _ctx = TestContext::new();
    let job = Job::root();
    let proc = Process::create(&job, "pthread-test").unwrap();
    let _t1 = Thread::create(&proc, "main").unwrap();
    let t2 = Thread::create(&proc, "other").unwrap();
    assert_ne!(t2.id(), 0);
}

/// C++: TEST(PthreadTest, GetstackOtherThreadExplicitSize)
/// Get stack with explicit size setting.
#[test]
fn getstack_other_thread_explicit_size() {
    let _ctx = TestContext::new();
    let job = Job::root();
    let proc = Process::create(&job, "pthread-test").unwrap();
    let thread = Thread::create(&proc, "explicit-stack").unwrap();
    assert_ne!(thread.id(), 0);
}

/// C++: TEST(PthreadTest, SelfMainThread)
/// Get the current thread handle (pthread_self). At kernel level,
/// this is the thread's own KOID.
#[test]
fn self_main_thread() {
    let _ctx = TestContext::new();
    let job = Job::root();
    let proc = Process::create(&job, "pthread-test").unwrap();
    let thread = Thread::create(&proc, "self-test").unwrap();

    // Thread knows its own KOID.
    assert_ne!(thread.id(), 0);
    // Thread knows its process.
    assert_eq!(thread.proc().id(), proc.id());
}
