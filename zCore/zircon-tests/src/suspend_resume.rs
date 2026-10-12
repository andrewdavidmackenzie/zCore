//! Suspend and resume tests.
//!
//! Ports of Fuchsia's suspend/resume tests for threads and processes.
//! Thread suspend/resume IS implemented via `SuspendToken` and the
//! `Task::suspend()`/`Task::resume()` methods, testable with the
//! async `test_loop_thread_fn` infrastructure.

use crate::helpers;
use zircon_object::object::{KernelObject, Signal};
use zircon_object::task::{Job, Process, SuspendToken, Task, Thread, ThreadState, ThreadStateKind};

extern crate alloc;
use alloc::sync::Arc;

/// Helper: cast Arc<Thread> to Arc<dyn KernelObject> for wait_signal.
fn as_kobject(thread: &Arc<Thread>) -> Arc<dyn KernelObject> {
    thread.clone()
}

///
/// Suspend a running thread. The thread should stop executing and
/// signal THREAD_SUSPENDED.
#[async_std::test]
async fn suspend_thread() {
    helpers::ensure_hal_init();
    let job = Job::root();
    let proc = Process::create(&job, "proc").unwrap();
    let thread = Thread::create(&proc, "t1").unwrap();

    thread.start(helpers::test_loop_thread_fn).unwrap();
    async_std::task::yield_now().await;
    async_std::task::yield_now().await;

    // Suspend the thread.
    thread.suspend();
    as_kobject(&thread)
        .wait_signal(Signal::THREAD_SUSPENDED)
        .await;

    assert!(thread.signal().contains(Signal::THREAD_SUSPENDED));
    assert!(!thread.signal().contains(Signal::THREAD_RUNNING));
    assert_eq!(thread.state(), ThreadState::Suspended);

    // Clean up.
    thread.resume();
    thread.kill();
    as_kobject(&thread)
        .wait_signal(Signal::THREAD_TERMINATED)
        .await;
}

///
/// Suspend then resume a thread. The thread should continue
/// executing from where it stopped.
#[async_std::test]
async fn resume_thread() {
    helpers::ensure_hal_init();
    let job = Job::root();
    let proc = Process::create(&job, "proc").unwrap();
    let thread = Thread::create(&proc, "t1").unwrap();

    thread.start(helpers::test_loop_thread_fn).unwrap();
    async_std::task::yield_now().await;
    async_std::task::yield_now().await;

    // Suspend.
    thread.suspend();
    as_kobject(&thread)
        .wait_signal(Signal::THREAD_SUSPENDED)
        .await;
    assert!(thread.signal().contains(Signal::THREAD_SUSPENDED));

    // Resume.
    thread.resume();
    as_kobject(&thread)
        .wait_signal(Signal::THREAD_RUNNING)
        .await;
    assert!(thread.signal().contains(Signal::THREAD_RUNNING));
    assert!(!thread.signal().contains(Signal::THREAD_SUSPENDED));

    // Clean up.
    thread.kill();
    as_kobject(&thread)
        .wait_signal(Signal::THREAD_TERMINATED)
        .await;
}

///
/// Suspend multiple threads in the same process simultaneously.
/// All should stop and be resumable.
#[async_std::test]
async fn suspend_multiple_threads() {
    helpers::ensure_hal_init();
    let job = Job::root();
    let proc = Process::create(&job, "proc").unwrap();
    let t1 = Thread::create(&proc, "t1").unwrap();
    let t2 = Thread::create(&proc, "t2").unwrap();

    t1.start(helpers::test_loop_thread_fn).unwrap();
    t2.start(helpers::test_loop_thread_fn).unwrap();
    async_std::task::yield_now().await;
    async_std::task::yield_now().await;

    // Suspend both.
    t1.suspend();
    t2.suspend();
    as_kobject(&t1).wait_signal(Signal::THREAD_SUSPENDED).await;
    as_kobject(&t2).wait_signal(Signal::THREAD_SUSPENDED).await;

    assert!(t1.signal().contains(Signal::THREAD_SUSPENDED));
    assert!(t2.signal().contains(Signal::THREAD_SUSPENDED));

    // Resume both.
    t1.resume();
    t2.resume();
    as_kobject(&t1).wait_signal(Signal::THREAD_RUNNING).await;
    as_kobject(&t2).wait_signal(Signal::THREAD_RUNNING).await;

    // Clean up.
    t1.kill();
    t2.kill();
    as_kobject(&t1).wait_signal(Signal::THREAD_TERMINATED).await;
    as_kobject(&t2).wait_signal(Signal::THREAD_TERMINATED).await;
}

///
/// Drop a suspend token. The thread should automatically resume.
#[async_std::test]
async fn suspend_token_drop() {
    helpers::ensure_hal_init();
    let job = Job::root();
    let proc = Process::create(&job, "proc").unwrap();
    let thread = Thread::create(&proc, "t1").unwrap();

    thread.start(helpers::test_loop_thread_fn).unwrap();
    async_std::task::yield_now().await;
    async_std::task::yield_now().await;

    // Create a SuspendToken -- this calls thread.suspend().
    let task: Arc<dyn Task> = thread.clone();
    let token = SuspendToken::create(&task);
    as_kobject(&thread)
        .wait_signal(Signal::THREAD_SUSPENDED)
        .await;
    assert!(thread.signal().contains(Signal::THREAD_SUSPENDED));

    // Drop the token -- thread should automatically resume.
    drop(token);
    as_kobject(&thread)
        .wait_signal(Signal::THREAD_RUNNING)
        .await;
    assert!(thread.signal().contains(Signal::THREAD_RUNNING));

    // Clean up.
    thread.kill();
    as_kobject(&thread)
        .wait_signal(Signal::THREAD_TERMINATED)
        .await;
}

///
/// Issue multiple suspend tokens for the same thread. The thread
/// should resume only after all tokens are dropped.
///
/// Note: zCore's current suspend/resume uses a counter, so multiple
/// suspends require matching resumes. We test via SuspendToken.
#[async_std::test]
async fn multiple_suspend_tokens() {
    helpers::ensure_hal_init();
    let job = Job::root();
    let proc = Process::create(&job, "proc").unwrap();
    let thread = Thread::create(&proc, "t1").unwrap();

    thread.start(helpers::test_loop_thread_fn).unwrap();
    async_std::task::yield_now().await;
    async_std::task::yield_now().await;

    let task: Arc<dyn Task> = thread.clone();
    let token1 = SuspendToken::create(&task);
    let token2 = SuspendToken::create(&task);
    as_kobject(&thread)
        .wait_signal(Signal::THREAD_SUSPENDED)
        .await;
    assert!(thread.signal().contains(Signal::THREAD_SUSPENDED));

    // Drop first token -- thread should still be suspended because
    // the second token is still alive.
    drop(token1);
    // Yield to let any pending state changes propagate.
    async_std::task::yield_now().await;
    // Thread should still be suspended (second token holds it).
    assert!(
        thread.signal().contains(Signal::THREAD_SUSPENDED),
        "thread should remain suspended with one token still active"
    );

    // Drop second token -- now the thread should resume.
    drop(token2);
    as_kobject(&thread)
        .wait_signal(Signal::THREAD_RUNNING)
        .await;
    assert!(thread.signal().contains(Signal::THREAD_RUNNING));

    // Clean up.
    thread.kill();
    as_kobject(&thread)
        .wait_signal(Signal::THREAD_TERMINATED)
        .await;
}

///
/// Kill a suspended thread. The thread should die without resuming.
#[async_std::test]
async fn kill_while_suspended() {
    helpers::ensure_hal_init();
    let job = Job::root();
    let proc = Process::create(&job, "proc").unwrap();
    let thread = Thread::create(&proc, "t1").unwrap();

    thread.start(helpers::test_loop_thread_fn).unwrap();
    async_std::task::yield_now().await;
    async_std::task::yield_now().await;

    // Suspend the thread.
    thread.suspend();
    as_kobject(&thread)
        .wait_signal(Signal::THREAD_SUSPENDED)
        .await;
    assert!(thread.signal().contains(Signal::THREAD_SUSPENDED));

    // Kill while suspended -- should terminate directly.
    thread.kill();
    as_kobject(&thread)
        .wait_signal(Signal::THREAD_TERMINATED)
        .await;
    assert!(thread.signal().contains(Signal::THREAD_TERMINATED));
}

///
/// While a thread is suspended, its state should be SUSPENDED.
/// Read state should succeed.
#[async_std::test]
async fn suspended_thread_state() {
    helpers::ensure_hal_init();
    let job = Job::root();
    let proc = Process::create(&job, "proc").unwrap();
    let thread = Thread::create(&proc, "t1").unwrap();

    thread.start(helpers::test_loop_thread_fn).unwrap();
    async_std::task::yield_now().await;
    async_std::task::yield_now().await;

    thread.suspend();
    as_kobject(&thread)
        .wait_signal(Signal::THREAD_SUSPENDED)
        .await;

    // State should be Suspended.
    assert_eq!(thread.state(), ThreadState::Suspended);

    // Read register state should succeed on a suspended thread.
    let mut buf = vec![0u8; 1024];
    let result = thread.read_state(ThreadStateKind::General, &mut buf);
    assert!(
        result.is_ok(),
        "read_state should succeed on a suspended thread"
    );

    // Clean up.
    thread.resume();
    thread.kill();
    as_kobject(&thread)
        .wait_signal(Signal::THREAD_TERMINATED)
        .await;
}

///
/// Write to a thread's register state while it is suspended.
/// The new state should take effect when the thread resumes.
#[test]
#[ignore = "write_state while suspended needs arch-specific register validation"]
fn write_state_while_suspended() {
    todo!("write_state while suspended needs arch-specific register validation");
}

///
/// Suspend all threads in a process simultaneously.
#[test]
#[ignore = "process-level suspend not fully testable in libos mode"]
fn suspend_process() {
    todo!("process-level suspend not fully testable in libos mode");
}
