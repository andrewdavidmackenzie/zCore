//! Thread tests.
//!
//! Tests for thread creation, lifecycle, register state, and signals.
//! Async tests use `test_loop_thread_fn` which simulates the real
//! userspace execution loop for thread start/suspend/resume testing.

use crate::helpers::{self, TestContext};
use alloc::sync::Arc;
use zircon_object::object::{KernelObject, Signal};
use zircon_object::task::{Job, Process, Task, Thread, ThreadState, ThreadStateKind};

extern crate alloc;

#[test]
fn thread_create() {
    let _ctx = TestContext::new();
    let job = Job::root();
    let proc = Process::create(&job, "proc").unwrap();
    let thread = Thread::create(&proc, "thread-1").unwrap();
    assert_ne!(thread.id(), 0);
}

#[test]
fn thread_name() {
    let _ctx = TestContext::new();
    let job = Job::root();
    let proc = Process::create(&job, "proc").unwrap();
    let thread = Thread::create(&proc, "my-thread").unwrap();
    assert_eq!(thread.name(), "my-thread");
}

#[test]
fn thread_related_koid() {
    let _ctx = TestContext::new();
    let job = Job::root();
    let proc = Process::create(&job, "proc").unwrap();
    let thread = Thread::create(&proc, "t1").unwrap();
    // Thread's related koid should be its process
    assert_eq!(thread.related_koid(), proc.id());
}

/// C++: TEST(ThreadTest, MultipleThreadsInProcess)
#[test]
fn thread_multiple_in_process() {
    let _ctx = TestContext::new();
    let job = Job::root();
    let proc = Process::create(&job, "proc").unwrap();

    let t1 = Thread::create(&proc, "t1").unwrap();
    let t2 = Thread::create(&proc, "t2").unwrap();
    let t3 = Thread::create(&proc, "t3").unwrap();

    assert_ne!(t1.id(), t2.id());
    assert_ne!(t2.id(), t3.id());
    assert_ne!(t1.id(), t3.id());

    // All threads belong to the same process
    assert_eq!(t1.related_koid(), proc.id());
    assert_eq!(t2.related_koid(), proc.id());
    assert_eq!(t3.related_koid(), proc.id());
}

/// C++: TEST(ThreadTest, SetName)
#[test]
fn thread_set_name() {
    let _ctx = TestContext::new();
    let job = Job::root();
    let proc = Process::create(&job, "proc").unwrap();
    let thread = Thread::create(&proc, "original").unwrap();

    assert_eq!(thread.name(), "original");
    thread.set_name("renamed");
    assert_eq!(thread.name(), "renamed");
}

/// C++: TEST(ThreadTest, TypeName)
#[test]
fn thread_type_name() {
    let _ctx = TestContext::new();
    let job = Job::root();
    let proc = Process::create(&job, "proc").unwrap();
    let thread = Thread::create(&proc, "t1").unwrap();

    assert_eq!(thread.type_name(), "Thread");
}

// ── Thread lifecycle tests (async, using test_loop_thread_fn) ───────

/// Helper: cast Arc<Thread> to Arc<dyn KernelObject> for wait_signal.
fn as_kobject(thread: &alloc::sync::Arc<Thread>) -> alloc::sync::Arc<dyn KernelObject> {
    thread.clone()
}

/// Thread starts and eventually terminates when killed.
#[async_std::test]
async fn thread_start_and_terminate() {
    helpers::ensure_hal_init();
    let job = Job::root();
    let proc = Process::create(&job, "proc").unwrap();
    let thread = Thread::create(&proc, "t1").unwrap();

    // Initially in New state.
    assert_eq!(thread.state(), ThreadState::New);

    // Start the thread.
    thread.start(helpers::test_loop_thread_fn).unwrap();

    // Give the thread a chance to run.
    async_std::task::yield_now().await;
    assert!(
        thread.signal().contains(Signal::THREAD_RUNNING),
        "thread should be running"
    );

    // Kill the thread.
    thread.kill();

    // Wait for termination.
    as_kobject(&thread)
        .wait_signal(Signal::THREAD_TERMINATED)
        .await;
    assert!(
        thread.signal().contains(Signal::THREAD_TERMINATED),
        "thread should be terminated"
    );
}

/// Thread can be suspended and resumed.
#[async_std::test]
async fn thread_suspend_resume() {
    helpers::ensure_hal_init();
    let job = Job::root();
    let proc = Process::create(&job, "proc").unwrap();
    let thread = Thread::create(&proc, "t1").unwrap();

    thread.start(helpers::test_loop_thread_fn).unwrap();

    // Give the thread time to enter the loop and put_context.
    async_std::task::yield_now().await;
    async_std::task::yield_now().await;

    // Suspend the thread.
    thread.suspend();

    // Wait for THREAD_SUSPENDED signal.
    as_kobject(&thread)
        .wait_signal(Signal::THREAD_SUSPENDED)
        .await;

    assert!(
        thread.signal().contains(Signal::THREAD_SUSPENDED),
        "thread should be suspended"
    );
    assert!(
        !thread.signal().contains(Signal::THREAD_RUNNING),
        "running signal should be cleared when suspended"
    );

    // Resume the thread.
    thread.resume();

    // Wait for THREAD_RUNNING signal again.
    as_kobject(&thread)
        .wait_signal(Signal::THREAD_RUNNING)
        .await;

    assert!(
        thread.signal().contains(Signal::THREAD_RUNNING),
        "thread should be running again"
    );
    assert!(
        !thread.signal().contains(Signal::THREAD_SUSPENDED),
        "suspended signal should be cleared"
    );

    // Clean up — kill the thread.
    thread.kill();
    as_kobject(&thread)
        .wait_signal(Signal::THREAD_TERMINATED)
        .await;
}

/// Read and write register state on a suspended thread.
#[async_std::test]
async fn thread_read_write_state() {
    helpers::ensure_hal_init();
    let job = Job::root();
    let proc = Process::create(&job, "proc").unwrap();
    let thread = Thread::create(&proc, "t1").unwrap();

    // Set up initial register state before starting.
    thread
        .with_context(|ctx| ctx.setup_uspace(0x1000, 0x8000, &[42, 99, 0]))
        .unwrap();

    thread.start(helpers::test_loop_thread_fn).unwrap();

    // Give the thread time to enter the loop.
    async_std::task::yield_now().await;
    async_std::task::yield_now().await;

    // Suspend to enable read_state/write_state.
    thread.suspend();
    as_kobject(&thread)
        .wait_signal(Signal::THREAD_SUSPENDED)
        .await;

    // Read register state (GeneralRegs).
    let mut buf = vec![0u8; 1024]; // large enough for any arch
    let bytes_read = thread
        .read_state(ThreadStateKind::General, &mut buf)
        .unwrap();
    assert!(bytes_read > 0, "should read some register state");

    // Write register state back (no-op modification, verifies round-trip).
    thread
        .write_state(ThreadStateKind::General, &buf[..bytes_read])
        .unwrap();

    // Resume and clean up.
    thread.resume();
    thread.kill();
    as_kobject(&thread)
        .wait_signal(Signal::THREAD_TERMINATED)
        .await;
}

/// read_state fails with BAD_STATE on a non-suspended thread.
#[test]
fn thread_read_state_not_suspended_fails() {
    let _ctx = TestContext::new();
    let job = Job::root();
    let proc = Process::create(&job, "proc").unwrap();
    let thread = Thread::create(&proc, "t1").unwrap();

    // Thread is in New state — read_state should fail.
    let mut buf = vec![0u8; 256];
    let result = thread.read_state(ThreadStateKind::General, &mut buf);
    assert!(result.is_err(), "read_state on New thread should fail");
}

// NOTE: Double-start test omitted — zCore's Thread::start() doesn't
// currently check for already-running state (Fuchsia returns BAD_STATE).
// This is a known gap, not a test infrastructure issue.
