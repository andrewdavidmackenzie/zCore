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
async fn suspend_thread() {
    helpers::ensure_hal_init();
    let job = Job::new_parentless_job();
    let proc = Process::create(&job, "proc").unwrap();
    let thread = Thread::create(&proc, "t1").unwrap();

    thread.start(helpers::test_loop_thread_fn).unwrap();
    hal_impl::thread::yield_now().await;
    hal_impl::thread::yield_now().await;

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
async fn resume_thread() {
    helpers::ensure_hal_init();
    let job = Job::new_parentless_job();
    let proc = Process::create(&job, "proc").unwrap();
    let thread = Thread::create(&proc, "t1").unwrap();

    thread.start(helpers::test_loop_thread_fn).unwrap();
    hal_impl::thread::yield_now().await;
    hal_impl::thread::yield_now().await;

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
async fn suspend_multiple_threads() {
    helpers::ensure_hal_init();
    let job = Job::new_parentless_job();
    let proc = Process::create(&job, "proc").unwrap();
    let t1 = Thread::create(&proc, "t1").unwrap();
    let t2 = Thread::create(&proc, "t2").unwrap();

    t1.start(helpers::test_loop_thread_fn).unwrap();
    t2.start(helpers::test_loop_thread_fn).unwrap();
    hal_impl::thread::yield_now().await;
    hal_impl::thread::yield_now().await;

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
async fn suspend_token_drop() {
    helpers::ensure_hal_init();
    let job = Job::new_parentless_job();
    let proc = Process::create(&job, "proc").unwrap();
    let thread = Thread::create(&proc, "t1").unwrap();

    thread.start(helpers::test_loop_thread_fn).unwrap();
    hal_impl::thread::yield_now().await;
    hal_impl::thread::yield_now().await;

    // Create a SuspendToken -- this calls thread.suspend().
    let token = SuspendToken::create_for_thread(&thread);
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
async fn multiple_suspend_tokens() {
    helpers::ensure_hal_init();
    let job = Job::new_parentless_job();
    let proc = Process::create(&job, "proc").unwrap();
    let thread = Thread::create(&proc, "t1").unwrap();

    thread.start(helpers::test_loop_thread_fn).unwrap();
    hal_impl::thread::yield_now().await;
    hal_impl::thread::yield_now().await;

    let token1 = SuspendToken::create_for_thread(&thread);
    let token2 = SuspendToken::create_for_thread(&thread);
    as_kobject(&thread)
        .wait_signal(Signal::THREAD_SUSPENDED)
        .await;
    assert!(thread.signal().contains(Signal::THREAD_SUSPENDED));

    // Drop first token -- thread should still be suspended because
    // the second token is still alive.
    drop(token1);
    // Yield to let any pending state changes propagate.
    hal_impl::thread::yield_now().await;
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
async fn kill_while_suspended() {
    helpers::ensure_hal_init();
    let job = Job::new_parentless_job();
    let proc = Process::create(&job, "proc").unwrap();
    let thread = Thread::create(&proc, "t1").unwrap();

    thread.start(helpers::test_loop_thread_fn).unwrap();
    hal_impl::thread::yield_now().await;
    hal_impl::thread::yield_now().await;

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
async fn suspended_thread_state() {
    helpers::ensure_hal_init();
    let job = Job::new_parentless_job();
    let proc = Process::create(&job, "proc").unwrap();
    let thread = Thread::create(&proc, "t1").unwrap();

    thread.start(helpers::test_loop_thread_fn).unwrap();
    hal_impl::thread::yield_now().await;
    hal_impl::thread::yield_now().await;

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
async fn write_state_while_suspended() {
    helpers::ensure_hal_init();
    let job = Job::new_parentless_job();
    let proc = Process::create(&job, "proc").unwrap();
    let thread = Thread::create(&proc, "t1").unwrap();

    thread.start(helpers::test_loop_thread_fn).unwrap();
    hal_impl::thread::yield_now().await;
    hal_impl::thread::yield_now().await;

    // Suspend the thread.
    thread.suspend();
    as_kobject(&thread)
        .wait_signal(Signal::THREAD_SUSPENDED)
        .await;

    // Read the general register state.
    let mut buf = vec![0u8; 1024];
    let size = thread
        .read_state(ThreadStateKind::General, &mut buf)
        .unwrap();
    assert!(size > 0, "register state should have non-zero size");

    // Write state back (no-op modification — just verify the API works).
    thread
        .write_state(ThreadStateKind::General, &buf[..size])
        .unwrap();

    // Resume and clean up.
    thread.resume();
    thread.kill();
    as_kobject(&thread)
        .wait_signal(Signal::THREAD_TERMINATED)
        .await;
}

///
/// Suspend all threads in a process simultaneously.
async fn suspend_process() {
    helpers::ensure_hal_init();
    let job = Job::new_parentless_job();
    let proc = Process::create(&job, "proc").unwrap();
    let t1 = Thread::create(&proc, "t1").unwrap();
    let t2 = Thread::create(&proc, "t2").unwrap();

    t1.start(helpers::test_loop_thread_fn).unwrap();
    t2.start(helpers::test_loop_thread_fn).unwrap();
    hal_impl::thread::yield_now().await;
    hal_impl::thread::yield_now().await;

    // Suspend the entire process.
    use zircon_object::task::Task;
    proc.suspend();
    as_kobject(&t1).wait_signal(Signal::THREAD_SUSPENDED).await;
    as_kobject(&t2).wait_signal(Signal::THREAD_SUSPENDED).await;

    assert!(t1.signal().contains(Signal::THREAD_SUSPENDED));
    assert!(t2.signal().contains(Signal::THREAD_SUSPENDED));

    // Resume the process.
    proc.resume();
    as_kobject(&t1).wait_signal(Signal::THREAD_RUNNING).await;
    as_kobject(&t2).wait_signal(Signal::THREAD_RUNNING).await;

    // Clean up.
    t1.kill();
    t2.kill();
    as_kobject(&t1).wait_signal(Signal::THREAD_TERMINATED).await;
    as_kobject(&t2).wait_signal(Signal::THREAD_TERMINATED).await;
}

/// C++: TEST(ProcessTest, SuspendWithDyingThread) — recipe; this is a
/// zCore-original stress regression test for issue #834.
///
/// Issue #834 regression: suspend/resume/exit must not deadlock
/// against concurrently terminating threads.
///
/// `Thread::terminate()` holds the thread lock while calling
/// `Process::remove_thread` (process lock). `Process::suspend()`,
/// `resume()` and `exit()` used to hold the process lock while
/// calling `Thread::suspend()`/`resume()`/`kill()` (thread lock) —
/// an ABBA pattern between two interrupt-disabling spinlocks. When
/// a dying thread's `terminate()` raced a process suspend (the
/// `ProcessTest.SuspendWithDyingThread` recipe: one thread exits
/// while the process is suspended), both CPUs spun forever with
/// interrupts disabled, freezing their executors: the test thread
/// never returned from the suspend syscall, parked threads never
/// received wake-ups, and the machine hung.
///
/// This test drives many rounds of that recipe — workers dying
/// while the process is suspended, resumed, and exited. A regression
/// hangs the test runner (the deadlock leaves the suspended workers'
/// wakers unfired, so `PROCESS_TERMINATED` never arrives).
async fn suspend_resume_race_with_dying_threads() {
    helpers::ensure_hal_init();
    const ROUNDS: usize = 30;
    for _round in 0..ROUNDS {
        let job = Job::new_parentless_job();
        let proc = Process::create(&job, "race-proc").unwrap();
        // Workers that terminate quickly; their `terminate()` →
        // `remove_thread()` (thread lock → process lock) must be
        // able to proceed while this task holds the process lock in
        // suspend/resume/exit below.
        let mut workers = alloc::vec::Vec::new();
        for _ in 0..4 {
            let t = Thread::create(&proc, "worker").unwrap();
            t.start(helpers::test_loop_thread_fn).unwrap();
            workers.push(t);
        }
        // Let the workers reach their run loops.
        for _ in 0..2 {
            hal_impl::thread::yield_now().await;
        }
        // Kill half the workers so their terminate() runs on other
        // CPUs exactly while this CPU suspends/resumes the process —
        // the SuspendWithDyingThread interleaving.
        for t in &workers[..2] {
            t.kill();
        }
        let token = SuspendToken::create_for_process(&proc);
        // Wait for the survivors to actually park suspended, then
        // resume (the suspend-token drop path) while the killed
        // workers are still terminating.
        for t in &workers[2..] {
            as_kobject(t).wait_signal(Signal::THREAD_SUSPENDED).await;
        }
        drop(token);
        for t in &workers[2..] {
            as_kobject(t).wait_signal(Signal::THREAD_RUNNING).await;
        }
        // Exit races any terminate() still in flight: the kill loop
        // must not spin on a thread lock held by a terminator waiting
        // for this process's lock.
        Process::exit(&proc, 0);
        // Every thread must terminate and the process must reach
        // PROCESS_TERMINATED — a lost wake or deadlock makes this
        // wait hang.
        for t in &workers {
            as_kobject(t).wait_signal(Signal::THREAD_TERMINATED).await;
        }
        as_kobject_proc(&proc)
            .wait_signal(Signal::PROCESS_TERMINATED)
            .await;
    }
}

/// Helper: cast Arc<Process> to Arc<dyn KernelObject> for wait_signal.
fn as_kobject_proc(proc: &Arc<Process>) -> Arc<dyn KernelObject> {
    proc.clone()
}

/// Run all tests in this module.
pub async fn run_all() {
    suspend_thread().await;
    resume_thread().await;
    suspend_multiple_threads().await;
    suspend_token_drop().await;
    multiple_suspend_tokens().await;
    kill_while_suspended().await;
    suspended_thread_state().await;
    write_state_while_suspended().await;
    suspend_process().await;
    suspend_resume_race_with_dying_threads().await;
}
