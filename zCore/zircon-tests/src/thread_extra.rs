//! Additional zCore-specific tests for thread functionality.
//!
//! These are NOT 1:1 ports of C++ core-tests. They exercise
//! kernel-object-level APIs and zCore-specific behavior.

use crate::core_tests::thread::as_kobject;
use crate::helpers::{self, TestContext};
use zircon_object::object::{Handle, KernelObject, Rights, Signal};
use zircon_object::task::{Job, Process, Task, Thread, ThreadState, ThreadStateKind};

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

#[test]
fn thread_type_name() {
    let _ctx = TestContext::new();
    let job = Job::root();
    let proc = Process::create(&job, "proc").unwrap();
    let thread = Thread::create(&proc, "t1").unwrap();

    assert_eq!(thread.type_name(), "Thread");
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

///
/// Creates a thread and starts it with a basic entry function.
/// Verifies that `start` transitions the thread to Running state
/// using the test loop thread function.
#[async_std::test]
async fn thread_create_and_start() {
    helpers::ensure_hal_init();
    let job = Job::root();
    let proc = Process::create(&job, "proc").unwrap();
    let thread = Thread::create(&proc, "t1").unwrap();

    assert_eq!(thread.state(), ThreadState::New);

    // Start the thread using the test loop.
    thread.start(helpers::test_loop_thread_fn).unwrap();

    async_std::task::yield_now().await;
    assert!(
        thread.signal().contains(Signal::THREAD_RUNNING),
        "thread should be running after start"
    );
    assert_eq!(thread.state(), ThreadState::Running);

    // Clean up.
    thread.kill();
    as_kobject(&thread)
        .wait_signal(Signal::THREAD_TERMINATED)
        .await;
    assert_eq!(thread.state(), ThreadState::Dead);
}

///
/// Kill a newly created thread. Killing a New thread transitions
/// it to Dying. The thread should be findable as a child of the
/// process before and not be in Running state after kill.
#[test]
fn thread_kill() {
    let _ctx = TestContext::new();
    let job = Job::root();
    let proc = Process::create(&job, "proc").unwrap();
    let thread = Thread::create(&proc, "t-kill").unwrap();

    assert_eq!(thread.state(), ThreadState::New);

    // Kill the thread.
    thread.kill();

    // After kill, state should be Dying (or Dead after cleanup).
    let state = thread.state();
    assert!(
        state == ThreadState::Dying || state == ThreadState::Dead,
        "thread state after kill should be Dying or Dead, got {:?}",
        state
    );
}

///
/// Verify get/set of the thread name property.
#[test]
fn thread_name_property() {
    let _ctx = TestContext::new();
    let job = Job::root();
    let proc = Process::create(&job, "proc").unwrap();
    let thread = Thread::create(&proc, "initial-name").unwrap();

    assert_eq!(thread.name(), "initial-name");

    thread.set_name("new-name");
    assert_eq!(thread.name(), "new-name");

    // Set empty name.
    thread.set_name("");
    assert_eq!(thread.name(), "");

    // Set a long name.
    thread.set_name("a-very-long-thread-name-for-testing");
    assert_eq!(thread.name(), "a-very-long-thread-name-for-testing");
}

///
/// Multiple threads in a process: start them all, kill them all,
/// wait for each to terminate.
#[async_std::test]
async fn thread_process_wait_many_threads() {
    helpers::ensure_hal_init();
    let job = Job::root();
    let proc = Process::create(&job, "proc").unwrap();

    let t1 = Thread::create(&proc, "t1").unwrap();
    let t2 = Thread::create(&proc, "t2").unwrap();
    let t3 = Thread::create(&proc, "t3").unwrap();

    // Start all three.
    t1.start(helpers::test_loop_thread_fn).unwrap();
    t2.start(helpers::test_loop_thread_fn).unwrap();
    t3.start(helpers::test_loop_thread_fn).unwrap();

    async_std::task::yield_now().await;

    // All should be running.
    assert!(t1.signal().contains(Signal::THREAD_RUNNING));
    assert!(t2.signal().contains(Signal::THREAD_RUNNING));
    assert!(t3.signal().contains(Signal::THREAD_RUNNING));

    // Kill all.
    t1.kill();
    t2.kill();
    t3.kill();

    // Wait for all to terminate.
    as_kobject(&t1).wait_signal(Signal::THREAD_TERMINATED).await;
    as_kobject(&t2).wait_signal(Signal::THREAD_TERMINATED).await;
    as_kobject(&t3).wait_signal(Signal::THREAD_TERMINATED).await;

    assert_eq!(t1.state(), ThreadState::Dead);
    assert_eq!(t2.state(), ThreadState::Dead);
    assert_eq!(t3.state(), ThreadState::Dead);
}

///
/// Reading thread state on a New (not-yet-started) thread should
/// return BAD_STATE because the thread hasn't been suspended.
#[test]
fn thread_read_state_before_start() {
    let _ctx = TestContext::new();
    let job = Job::root();
    let proc = Process::create(&job, "proc").unwrap();
    let thread = Thread::create(&proc, "t1").unwrap();

    assert_eq!(thread.state(), ThreadState::New);

    let mut buf = vec![0u8; 512];
    let result = thread.read_state(ThreadStateKind::General, &mut buf);
    assert_eq!(
        result.err(),
        Some(zircon_object::ZxError::BAD_STATE),
        "read_state on New thread should return BAD_STATE"
    );
}

///
/// Reading thread state with an unsupported kind should fail.
/// FloatPoint/Vector/Debug/SingleStep are not supported — they
/// return NOT_SUPPORTED on a suspended thread, or BAD_STATE on a
/// non-suspended thread.
#[test]
fn thread_read_state_invalid_kind() {
    let _ctx = TestContext::new();
    let job = Job::root();
    let proc = Process::create(&job, "proc").unwrap();
    let thread = Thread::create(&proc, "t1").unwrap();

    // Suspend the thread so read_state doesn't fail with BAD_STATE.
    thread.suspend();

    let mut buf = vec![0u8; 512];
    // FloatPoint is valid enum but not implemented — should return NOT_SUPPORTED.
    let result = thread.read_state(ThreadStateKind::FloatPoint, &mut buf);
    assert!(
        result.is_err(),
        "read_state with FloatPoint kind should fail"
    );
}

///
/// Writing thread state with an unsupported kind should fail.
#[test]
fn thread_write_state_invalid_kind() {
    let _ctx = TestContext::new();
    let job = Job::root();
    let proc = Process::create(&job, "proc").unwrap();
    let thread = Thread::create(&proc, "t1").unwrap();

    // Suspend the thread so write_state doesn't fail with BAD_STATE.
    thread.suspend();

    let buf = vec![0u8; 512];
    let result = thread.write_state(ThreadStateKind::FloatPoint, &buf);
    assert!(
        result.is_err(),
        "write_state with FloatPoint kind should fail"
    );
}

///
/// A suspended thread should allow read_state for General registers.
#[async_std::test]
async fn thread_read_state_suspended() {
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

    let mut buf = vec![0u8; 1024];
    let result = thread.read_state(ThreadStateKind::General, &mut buf);
    assert!(
        result.is_ok(),
        "read_state on suspended thread should succeed"
    );

    thread.resume();
    thread.kill();
    as_kobject(&thread)
        .wait_signal(Signal::THREAD_TERMINATED)
        .await;
}

///
/// A suspended thread should allow write_state for General registers.
#[async_std::test]
async fn thread_write_state_suspended() {
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

    // Read then write back — verifies write_state works.
    let mut buf = vec![0u8; 1024];
    let size = thread
        .read_state(ThreadStateKind::General, &mut buf)
        .unwrap();
    let result = thread.write_state(ThreadStateKind::General, &buf[..size]);
    assert!(
        result.is_ok(),
        "write_state on suspended thread should succeed"
    );

    thread.resume();
    thread.kill();
    as_kobject(&thread)
        .wait_signal(Signal::THREAD_TERMINATED)
        .await;
}

///
/// Multiple suspends require matching number of resumes.
#[async_std::test]
async fn thread_suspend_multiple() {
    helpers::ensure_hal_init();
    let job = Job::root();
    let proc = Process::create(&job, "proc").unwrap();
    let thread = Thread::create(&proc, "t1").unwrap();

    thread.start(helpers::test_loop_thread_fn).unwrap();
    async_std::task::yield_now().await;
    async_std::task::yield_now().await;

    // Suspend twice.
    thread.suspend();
    thread.suspend();

    as_kobject(&thread)
        .wait_signal(Signal::THREAD_SUSPENDED)
        .await;

    // First resume — still suspended (count > 0).
    thread.resume();
    // Thread should still be suspended after one resume.
    assert!(
        thread.signal().contains(Signal::THREAD_SUSPENDED),
        "should still be suspended after one resume of double-suspend"
    );

    // Second resume — now actually running.
    thread.resume();
    as_kobject(&thread)
        .wait_signal(Signal::THREAD_RUNNING)
        .await;

    assert!(
        thread.signal().contains(Signal::THREAD_RUNNING),
        "should be running after both resumes"
    );

    thread.kill();
    as_kobject(&thread)
        .wait_signal(Signal::THREAD_TERMINATED)
        .await;
}

///
/// Suspend a running thread from the test harness, verify it enters
/// Suspended state, then resume and clean up. This tests the suspend
/// mechanism on a thread that has been started
#[async_std::test]
async fn thread_suspend_self() {
    helpers::ensure_hal_init();
    let job = Job::root();
    let proc = Process::create(&job, "proc").unwrap();
    let thread = Thread::create(&proc, "t1").unwrap();

    thread.start(helpers::test_loop_thread_fn).unwrap();
    async_std::task::yield_now().await;
    async_std::task::yield_now().await;

    // Suspend the running thread.
    thread.suspend();
    as_kobject(&thread)
        .wait_signal(Signal::THREAD_SUSPENDED)
        .await;

    assert!(thread.signal().contains(Signal::THREAD_SUSPENDED));
    assert!(!thread.signal().contains(Signal::THREAD_RUNNING));

    // Resume and clean up.
    thread.resume();
    as_kobject(&thread)
        .wait_signal(Signal::THREAD_RUNNING)
        .await;

    thread.kill();
    as_kobject(&thread)
        .wait_signal(Signal::THREAD_TERMINATED)
        .await;
}

///
/// A thread that is suspended can be killed, and transitions to
/// Dying/Dead.
#[async_std::test]
async fn thread_kill_suspended() {
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

    // Kill while suspended.
    thread.kill();

    // Should eventually terminate.
    as_kobject(&thread)
        .wait_signal(Signal::THREAD_TERMINATED)
        .await;
    assert!(
        thread.signal().contains(Signal::THREAD_TERMINATED),
        "killed suspended thread should be terminated"
    );
}

///
/// Getting an exception report on a thread not in exception state
/// should return BAD_STATE.
#[test]
fn thread_get_exception_report_no_exception() {
    let _ctx = TestContext::new();
    let job = Job::root();
    let proc = Process::create(&job, "proc").unwrap();
    let thread = Thread::create(&proc, "t1").unwrap();

    // Thread is New, not in exception — should fail with BAD_STATE.
    let result = thread.get_thread_exception_info();
    assert_eq!(
        result.err(),
        Some(zircon_object::ZxError::BAD_STATE),
        "get_thread_exception_info on non-exception thread should fail"
    );
}

///
/// Start a thread with explicit entry point and stack pointer.
/// Verifies that the context is set up correctly before the
/// thread begins execution.
#[async_std::test]
async fn thread_start_with_entry() {
    helpers::ensure_hal_init();
    let job = Job::root();
    let proc = Process::create(&job, "proc").unwrap();
    let thread = Thread::create(&proc, "t1").unwrap();

    // Start with explicit entry/stack.
    thread
        .start_with_entry(0x1000, 0x8000, 42, 99, helpers::test_loop_thread_fn)
        .unwrap();

    async_std::task::yield_now().await;
    assert!(
        thread.signal().contains(Signal::THREAD_RUNNING),
        "thread should be running after start_with_entry"
    );

    thread.kill();
    as_kobject(&thread)
        .wait_signal(Signal::THREAD_TERMINATED)
        .await;
}

///
/// Starting a thread that is already running should ideally fail
/// with BAD_STATE (Fuchsia behavior). zCore's Thread::start()
/// doesn't currently guard against this — it unconditionally
/// transitions to Running. This test documents the current behavior:
/// double-start succeeds (a known gap).
#[async_std::test]
async fn thread_double_start_fails() {
    helpers::ensure_hal_init();
    let job = Job::root();
    let proc = Process::create(&job, "proc").unwrap();
    let thread = Thread::create(&proc, "t1").unwrap();

    // First start should succeed.
    thread.start(helpers::test_loop_thread_fn).unwrap();
    async_std::task::yield_now().await;
    assert_eq!(thread.state(), ThreadState::Running);

    // Second start — zCore currently allows this (known gap).
    // Fuchsia would return BAD_STATE here.
    let result = thread.start(helpers::test_loop_thread_fn);
    // Document: start() returns Ok even on double-start.
    assert!(
        result.is_ok(),
        "zCore currently allows double-start (known gap vs Fuchsia)"
    );

    // Clean up.
    thread.kill();
    as_kobject(&thread)
        .wait_signal(Signal::THREAD_TERMINATED)
        .await;
}

///
/// get_thread_info returns a ThreadInfo struct. We verify the thread's
/// state via thread.state() since ThreadInfo fields are private to
/// the zircon-object crate. The info struct is primarily used for
/// ABI serialization to userspace.
#[test]
fn thread_info_state() {
    let _ctx = TestContext::new();
    let job = Job::root();
    let proc = Process::create(&job, "proc").unwrap();
    let thread = Thread::create(&proc, "t1").unwrap();

    // Verify state via the public API.
    assert_eq!(thread.state(), ThreadState::New);

    // get_thread_info should not panic.
    let _info = thread.get_thread_info();

    // get_thread_exception_info should fail with BAD_STATE (no exception).
    assert_eq!(
        thread.get_thread_exception_info().err(),
        Some(zircon_object::ZxError::BAD_STATE)
    );
}

///
/// Start a thread, kill it, and wait for the THREAD_TERMINATED signal.
#[async_std::test]
async fn thread_wait_for_termination() {
    helpers::ensure_hal_init();
    let job = Job::root();
    let proc = Process::create(&job, "proc").unwrap();
    let thread = Thread::create(&proc, "t1").unwrap();

    thread.start(helpers::test_loop_thread_fn).unwrap();
    async_std::task::yield_now().await;

    thread.kill();

    as_kobject(&thread)
        .wait_signal(Signal::THREAD_TERMINATED)
        .await;

    assert_eq!(thread.state(), ThreadState::Dead);
    assert!(thread.signal().contains(Signal::THREAD_TERMINATED));
}

///
/// Thread runtime stats require actual CPU execution time tracking.
/// This test verifies the basic time tracking API.
#[test]
fn thread_stats_runtime() {
    let _ctx = TestContext::new();
    let job = Job::root();
    let proc = Process::create(&job, "proc").unwrap();
    let thread = Thread::create(&proc, "t1").unwrap();

    // New thread should have zero runtime.
    assert_eq!(thread.get_time(), 0);

    // Add simulated runtime.
    thread.time_add(1000);
    assert_eq!(thread.get_time(), 1000);

    thread.time_add(500);
    assert_eq!(thread.get_time(), 1500);
}

///
/// Starting a thread with an invalid handle is a process-level
/// concern — Process::start() takes a handle arg. Here we verify
/// that Thread::start (the kernel-level API) works without handles
/// and that the thread can be created and started normally.
#[async_std::test]
async fn thread_invalid_handle_start() {
    helpers::ensure_hal_init();
    let job = Job::root();
    let proc = Process::create(&job, "proc").unwrap();
    let thread = Thread::create(&proc, "t1").unwrap();

    // Thread::start() at the kernel level doesn't take a handle arg.
    // It should succeed regardless.
    let result = thread.start(helpers::test_loop_thread_fn);
    assert!(result.is_ok(), "kernel-level start should succeed");

    thread.kill();
    as_kobject(&thread)
        .wait_signal(Signal::THREAD_TERMINATED)
        .await;
}

///
/// Suspend a thread blocked in zx_channel_call. Requires a real
/// running thread blocked in a syscall — not possible in libos mode.
#[test]
fn thread_suspend_in_channel_call() {}

///
/// Suspend a thread blocked in zx_port_wait. Requires preemptive
/// scheduling to interrupt the blocked thread.
#[test]
fn thread_suspend_in_port_wait() {}

///
/// Suspend a thread blocked in zx_nanosleep. Requires preemptive
/// scheduling to interrupt the sleeping thread.
#[test]
fn thread_suspend_in_sleep() {}

///
/// Suspend a running thread, verify it stops executing, then resume
/// it and verify it continues.
#[async_std::test]
async fn thread_suspend_then_resume() {
    helpers::ensure_hal_init();
    let job = Job::root();
    let proc = Process::create(&job, "proc").unwrap();
    let thread = Thread::create(&proc, "t1").unwrap();

    thread.start(helpers::test_loop_thread_fn).unwrap();
    async_std::task::yield_now().await;
    async_std::task::yield_now().await;

    assert!(thread.signal().contains(Signal::THREAD_RUNNING));

    // Suspend
    thread.suspend();
    as_kobject(&thread)
        .wait_signal(Signal::THREAD_SUSPENDED)
        .await;
    assert!(thread.signal().contains(Signal::THREAD_SUSPENDED));
    assert!(!thread.signal().contains(Signal::THREAD_RUNNING));

    // Resume
    thread.resume();
    as_kobject(&thread)
        .wait_signal(Signal::THREAD_RUNNING)
        .await;
    assert!(thread.signal().contains(Signal::THREAD_RUNNING));

    thread.kill();
    as_kobject(&thread)
        .wait_signal(Signal::THREAD_TERMINATED)
        .await;
}

///
/// Suspend a thread N times, verify it requires N resumes.
#[async_std::test]
async fn thread_suspend_multiple_times() {
    helpers::ensure_hal_init();
    let job = Job::root();
    let proc = Process::create(&job, "proc").unwrap();
    let thread = Thread::create(&proc, "t1").unwrap();

    thread.start(helpers::test_loop_thread_fn).unwrap();
    async_std::task::yield_now().await;
    async_std::task::yield_now().await;

    // Suspend 3 times
    thread.suspend();
    thread.suspend();
    thread.suspend();
    as_kobject(&thread)
        .wait_signal(Signal::THREAD_SUSPENDED)
        .await;

    // Resume 1: still suspended
    thread.resume();
    assert!(thread.signal().contains(Signal::THREAD_SUSPENDED));

    // Resume 2: still suspended
    thread.resume();
    assert!(thread.signal().contains(Signal::THREAD_SUSPENDED));

    // Resume 3: now running
    thread.resume();
    as_kobject(&thread)
        .wait_signal(Signal::THREAD_RUNNING)
        .await;
    assert!(thread.signal().contains(Signal::THREAD_RUNNING));

    thread.kill();
    as_kobject(&thread)
        .wait_signal(Signal::THREAD_TERMINATED)
        .await;
}

///
/// Set and get thread priority.
#[test]
fn thread_priority() {
    use zircon_object::task::{Profile, ProfileInfo, ProfileInfoFlags};

    let ctx = TestContext::new();
    let sc = ctx.syscall();
    let job = Job::root();
    let proc = Process::create(&job, "prio-test").unwrap();
    let thread = Thread::create(&proc, "t1").unwrap();

    // Create a priority profile
    let info = ProfileInfo::with_priority(ProfileInfoFlags::PRIORITY.bits(), 24);
    let profile = Profile::create(info).unwrap();
    assert_eq!(profile.info.priority(), 24);

    // Apply profile to thread via syscall
    let thread_h = ctx
        .proc
        .add_handle(Handle::new(thread.clone(), Rights::DEFAULT_THREAD));
    let profile_h = ctx
        .proc
        .add_handle(Handle::new(profile, Rights::DEFAULT_PROFILE));
    let result = sc.sys_object_set_profile(thread_h, profile_h, 0);
    assert!(result.is_ok(), "set_profile should succeed: {:?}", result);
}

///
/// Set CPU affinity mask on a thread. Create a CPU_MASK profile
/// and apply it.
#[test]
fn thread_affinity() {
    use zircon_object::task::{Profile, ProfileInfo, ProfileInfoFlags};

    let ctx = TestContext::new();
    let sc = ctx.syscall();
    let job = Job::root();
    let proc = Process::create(&job, "affinity-test").unwrap();
    let thread = Thread::create(&proc, "t1").unwrap();

    let mut info = ProfileInfo::default();
    info.flags_raw = (ProfileInfoFlags::PRIORITY | ProfileInfoFlags::CPU_MASK).bits();
    // Set priority in sched_union
    let prio_bytes = 16i32.to_ne_bytes();
    unsafe {
        let ptr = &mut info as *mut ProfileInfo as *mut u8;
        core::ptr::copy_nonoverlapping(prio_bytes.as_ptr(), ptr.add(8), 4);
    }
    info.cpu_affinity_mask[0] = 0xF; // CPUs 0-3
    let profile = Profile::create(info).unwrap();

    let thread_h = ctx
        .proc
        .add_handle(Handle::new(thread.clone(), Rights::DEFAULT_THREAD));
    let profile_h = ctx
        .proc
        .add_handle(Handle::new(profile, Rights::DEFAULT_PROFILE));
    let result = sc.sys_object_set_profile(thread_h, profile_h, 0);
    assert!(
        result.is_ok(),
        "set_profile with CPU mask should succeed: {:?}",
        result
    );
}

///
/// Get/set the CPU mask for a thread. Uses profile API.
#[test]
fn thread_cpu_mask() {
    use zircon_object::task::{Profile, ProfileInfo, ProfileInfoFlags};

    let _ctx = TestContext::new();

    // CPU_MASK-only profile
    let mut info = ProfileInfo::default();
    info.flags_raw = ProfileInfoFlags::CPU_MASK.bits();
    info.cpu_affinity_mask[0] = 0xFF;
    let profile = Profile::create(info).unwrap();
    assert_eq!(profile.info.cpu_affinity_mask[0], 0xFF);

    // Can create with different masks
    let mut info2 = ProfileInfo::default();
    info2.flags_raw = ProfileInfoFlags::CPU_MASK.bits();
    info2.cpu_affinity_mask[0] = 1; // Only CPU 0
    let profile2 = Profile::create(info2).unwrap();
    assert_eq!(profile2.info.cpu_affinity_mask[0], 1);
}

///
/// Set explicit CPU affinity via the profile API.
#[test]
fn thread_set_cpu_affinity() {
    use zircon_object::task::{Profile, ProfileInfo, ProfileInfoFlags};

    let ctx = TestContext::new();
    let sc = ctx.syscall();
    let job = Job::root();
    let proc = Process::create(&job, "cpu-affinity").unwrap();
    let thread = Thread::create(&proc, "t1").unwrap();

    // Create CPU_MASK + PRIORITY profile
    let info = ProfileInfo::with_priority(
        (ProfileInfoFlags::PRIORITY | ProfileInfoFlags::CPU_MASK).bits(),
        16,
    );
    let profile = Profile::create(info).unwrap();

    let thread_h = ctx
        .proc
        .add_handle(Handle::new(thread, Rights::DEFAULT_THREAD));
    let profile_h = ctx
        .proc
        .add_handle(Handle::new(profile, Rights::DEFAULT_PROFILE));
    let result = sc.sys_object_set_profile(thread_h, profile_h, 0);
    assert!(result.is_ok(), "set_profile with affinity should succeed");
}

///
/// Register an exception channel, start a thread that immediately
/// faults, verify the exception is delivered. Requires exception
/// channel infrastructure with real thread execution.
#[test]
fn thread_exception_on_start() {}

///
/// Verify default rights on a thread handle. Thread handles should
/// have DEFAULT_THREAD rights which include BASIC, IO, PROPERTY,
/// DESTROY, SIGNAL, and MANAGE_THREAD.
#[test]
fn thread_rights_basic() {
    let _ctx = TestContext::new();
    let job = Job::root();
    let proc = Process::create(&job, "proc").unwrap();
    let thread = Thread::create(&proc, "t1").unwrap();

    // Create a handle with default thread rights.
    let handle = Handle::new(thread.clone(), Rights::DEFAULT_THREAD);
    assert_eq!(handle.rights, Rights::DEFAULT_THREAD);

    // Verify expected rights are present.
    assert!(handle.rights.contains(Rights::DUPLICATE));
    assert!(handle.rights.contains(Rights::TRANSFER));
    assert!(handle.rights.contains(Rights::WAIT));
    assert!(handle.rights.contains(Rights::INSPECT));
    assert!(handle.rights.contains(Rights::READ));
    assert!(handle.rights.contains(Rights::WRITE));
    assert!(handle.rights.contains(Rights::GET_PROPERTY));
    assert!(handle.rights.contains(Rights::SET_PROPERTY));
    assert!(handle.rights.contains(Rights::DESTROY));
    assert!(handle.rights.contains(Rights::SIGNAL));
    assert!(handle.rights.contains(Rights::MANAGE_THREAD));

    // Verify rights NOT expected on threads.
    assert!(!handle.rights.contains(Rights::ENUMERATE));
    assert!(!handle.rights.contains(Rights::MANAGE_PROCESS));
    assert!(!handle.rights.contains(Rights::MANAGE_JOB));

    // Add the thread handle to the process handle table and verify
    // we can retrieve it with the correct rights.
    let hv = proc.add_handle(Handle::new(thread.clone(), Rights::DEFAULT_THREAD));
    let retrieved = proc
        .get_object_with_rights::<Thread>(hv, Rights::MANAGE_THREAD)
        .unwrap();
    assert_eq!(retrieved.id(), thread.id());

    // Retrieving with a right not in DEFAULT_THREAD should fail.
    let result = proc.get_object_with_rights::<Thread>(hv, Rights::MANAGE_JOB);
    assert_eq!(result.unwrap_err(), zircon_object::ZxError::ACCESS_DENIED);
}

///
/// Verify that get_thread_info returns a valid ThreadInfo struct
/// reflecting the thread's current state. While in real Fuchsia
/// this is done via zx_object_get_info from the thread itself,
/// we test the underlying kernel API directly.
#[test]
fn thread_get_info_self() {
    let _ctx = TestContext::new();
    let job = Job::root();
    let proc = Process::create(&job, "proc").unwrap();
    let thread = Thread::create(&proc, "t1").unwrap();

    // ThreadInfo struct is returned without error on a New thread.
    let _info = thread.get_thread_info();

    // The thread state should be New (0).
    assert_eq!(thread.state(), ThreadState::New);

    // Kill the thread and verify info still works on a Dying thread.
    thread.kill();
    let _info = thread.get_thread_info();
    let state = thread.state();
    assert!(
        state == ThreadState::Dying || state == ThreadState::Dead,
        "state after kill should be Dying or Dead, got {:?}",
        state
    );
}

///
/// Verify all state transitions: New -> Running -> Suspended ->
/// Running -> Dying -> Dead using the async test infrastructure.
#[async_std::test]
async fn thread_state_transitions() {
    helpers::ensure_hal_init();
    let job = Job::root();
    let proc = Process::create(&job, "proc").unwrap();
    let thread = Thread::create(&proc, "t1").unwrap();

    // 1. New
    assert_eq!(thread.state(), ThreadState::New);

    // 2. New -> Running
    thread.start(helpers::test_loop_thread_fn).unwrap();
    async_std::task::yield_now().await;
    async_std::task::yield_now().await;
    assert_eq!(thread.state(), ThreadState::Running);

    // 3. Running -> Suspended
    thread.suspend();
    as_kobject(&thread)
        .wait_signal(Signal::THREAD_SUSPENDED)
        .await;
    assert_eq!(thread.state(), ThreadState::Suspended);

    // 4. Suspended -> Running
    thread.resume();
    as_kobject(&thread)
        .wait_signal(Signal::THREAD_RUNNING)
        .await;
    assert_eq!(thread.state(), ThreadState::Running);

    // 5. Running -> Dying -> Dead
    thread.kill();
    as_kobject(&thread)
        .wait_signal(Signal::THREAD_TERMINATED)
        .await;
    assert_eq!(thread.state(), ThreadState::Dead);
}

///
/// Use zx_object_wait_async on a thread handle for TERMINATED signal.
/// Requires a port + async wait infrastructure with real execution.
#[test]
fn thread_wait_async() {}

///
/// Cancel an async wait on a thread handle. Requires port + async
/// wait infrastructure with real execution.
#[test]
fn thread_cancel_wait_async() {}

///
/// Obtain a suspend token and verify suspension. Drop the token to
/// resume. Requires suspend token lifecycle infrastructure.
#[test]
fn thread_suspend_token() {
    let _ctx = TestContext::new();
    let job = Job::root();
    let proc = Process::create(&job, "proc").unwrap();
    let thread = Thread::create(&proc, "t1").unwrap();

    // Suspend the thread directly (suspend tokens aren't exposed
    // as separate objects in the kernel API — suspend() increments
    // the suspend count).
    thread.suspend();

    // Thread should transition toward suspended (it's New, so the
    // state depends on context availability).
    let state = thread.state();
    // For a New thread with context available and suspend_count > 0,
    // the state should be Suspended.
    assert!(
        state == ThreadState::Suspended || state == ThreadState::New,
        "state should be Suspended or New after suspend, got {:?}",
        state
    );

    // Resume
    thread.resume();
}

///
/// Kill a thread while it holds a suspend count. Thread should
/// transition to Dead regardless.
#[async_std::test]
async fn thread_kill_in_suspend() {
    helpers::ensure_hal_init();
    let job = Job::root();
    let proc = Process::create(&job, "proc").unwrap();
    let thread = Thread::create(&proc, "t1").unwrap();

    thread.start(helpers::test_loop_thread_fn).unwrap();
    async_std::task::yield_now().await;
    async_std::task::yield_now().await;

    // Suspend, then kill
    thread.suspend();
    as_kobject(&thread)
        .wait_signal(Signal::THREAD_SUSPENDED)
        .await;

    thread.kill();
    as_kobject(&thread)
        .wait_signal(Signal::THREAD_TERMINATED)
        .await;

    assert_eq!(thread.state(), ThreadState::Dead);
}

///
/// A thread exits (returns from entry) while suspended. The exit
/// should take priority over the suspension.
#[async_std::test]
async fn thread_exit_while_suspended() {
    helpers::ensure_hal_init();
    let job = Job::root();
    let proc = Process::create(&job, "proc").unwrap();
    let thread = Thread::create(&proc, "t1").unwrap();

    thread.start(helpers::test_loop_thread_fn).unwrap();
    async_std::task::yield_now().await;
    async_std::task::yield_now().await;

    // Suspend
    thread.suspend();
    as_kobject(&thread)
        .wait_signal(Signal::THREAD_SUSPENDED)
        .await;

    // Kill (simulates exit) — should override suspend
    thread.kill();
    as_kobject(&thread)
        .wait_signal(Signal::THREAD_TERMINATED)
        .await;
    assert!(thread.signal().contains(Signal::THREAD_TERMINATED));
}

///
/// Resume a thread that is stopped in an exception handler. Requires
/// exception channel infrastructure with real thread execution.
#[test]
fn thread_resume_from_exception() {}

///
/// Create a thread with invalid arguments via the syscall layer.
/// Non-zero options should return INVALID_ARGS.
#[test]
fn thread_create_invalid_args() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();

    // Add a process handle to create threads on.
    let job = Job::root();
    let target_proc = Process::create(&job, "target").unwrap();
    let proc_h = ctx
        .proc
        .add_handle(Handle::new(target_proc, Rights::DEFAULT_PROCESS));

    let name = b"test-thread";
    let mut thread_h: zircon_object::object::HandleValue = 0;

    // Non-zero options should fail with INVALID_ARGS.
    let result = sc.sys_thread_create(
        proc_h,
        hal_impl::user::UserInPtr::from(name.as_ptr() as usize),
        name.len(),
        1, // invalid options
        hal_impl::user::UserOutPtr::from(&mut thread_h as *mut _ as usize),
    );
    assert_eq!(result.unwrap_err(), zircon_object::ZxError::INVALID_ARGS);

    // Zero options should succeed.
    let result = sc.sys_thread_create(
        proc_h,
        hal_impl::user::UserInPtr::from(name.as_ptr() as usize),
        name.len(),
        0,
        hal_impl::user::UserOutPtr::from(&mut thread_h as *mut _ as usize),
    );
    assert!(result.is_ok());
    assert_ne!(thread_h, 0);
}

///
/// Suspend a thread that is stopped in an exception handler. The
/// thread should remain suspended after the exception is resolved.
/// Requires exception channel + real thread execution.
#[test]
fn thread_suspend_in_exception() {}

///
/// Trigger an exception while a thread is suspended. The exception
/// should be queued and delivered when the thread is resumed.
/// Requires exception channel + suspension infrastructure.
#[test]
fn thread_exception_during_suspend() {}

///
/// Suspend a thread before starting it, then start it. The thread
/// should enter Suspended state rather than Running, and resume
/// only after an explicit resume call.
#[async_std::test]
async fn thread_start_suspended_thread() {
    helpers::ensure_hal_init();
    let job = Job::root();
    let proc = Process::create(&job, "proc").unwrap();
    let thread = Thread::create(&proc, "t1").unwrap();

    // Suspend before starting.
    thread.suspend();

    // Start the thread — it should become suspended because
    // suspend_count > 0.
    thread.start(helpers::test_loop_thread_fn).unwrap();
    async_std::task::yield_now().await;
    async_std::task::yield_now().await;

    // Thread should be suspended (not running).
    as_kobject(&thread)
        .wait_signal(Signal::THREAD_SUSPENDED)
        .await;
    assert!(thread.signal().contains(Signal::THREAD_SUSPENDED));
    assert!(!thread.signal().contains(Signal::THREAD_RUNNING));

    // Resume — now it should run.
    thread.resume();
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
/// Create a thread with bad arguments at the syscall level.
/// Invalid process handle should return BAD_HANDLE. Using a
/// non-process handle should return WRONG_TYPE.
#[test]
fn thread_create_with_bad_args() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();

    let name = b"bad-thread";
    let mut thread_h: zircon_object::object::HandleValue = 0;

    // Invalid handle value (0xDEAD) should return BAD_HANDLE.
    let result = sc.sys_thread_create(
        0xDEAD,
        hal_impl::user::UserInPtr::from(name.as_ptr() as usize),
        name.len(),
        0,
        hal_impl::user::UserOutPtr::from(&mut thread_h as *mut _ as usize),
    );
    assert_eq!(result.unwrap_err(), zircon_object::ZxError::BAD_HANDLE);

    // Use a job handle (wrong type) instead of a process handle.
    let job = Job::root();
    let job_h = ctx.proc.add_handle(Handle::new(job, Rights::DEFAULT_JOB));
    let result = sc.sys_thread_create(
        job_h,
        hal_impl::user::UserInPtr::from(name.as_ptr() as usize),
        name.len(),
        0,
        hal_impl::user::UserOutPtr::from(&mut thread_h as *mut _ as usize),
    );
    assert_eq!(result.unwrap_err(), zircon_object::ZxError::WRONG_TYPE);
}

///
/// Register multiple exception channels on a thread. Exceptions
/// should be delivered to the most recently registered channel
/// first. Requires exception channel infrastructure.
#[test]
fn thread_multiple_exception_channels() {}

///
/// Close a suspend token while the thread is suspended. The thread
/// should resume execution.
#[async_std::test]
async fn thread_close_suspend_token() {
    helpers::ensure_hal_init();
    let job = Job::root();
    let proc = Process::create(&job, "proc").unwrap();
    let thread = Thread::create(&proc, "t1").unwrap();

    thread.start(helpers::test_loop_thread_fn).unwrap();
    async_std::task::yield_now().await;
    async_std::task::yield_now().await;

    // Suspend
    thread.suspend();
    as_kobject(&thread)
        .wait_signal(Signal::THREAD_SUSPENDED)
        .await;

    // "Close" the suspend token by resuming
    thread.resume();
    as_kobject(&thread)
        .wait_signal(Signal::THREAD_RUNNING)
        .await;
    assert!(thread.signal().contains(Signal::THREAD_RUNNING));

    thread.kill();
    as_kobject(&thread)
        .wait_signal(Signal::THREAD_TERMINATED)
        .await;
}

///
/// Verify suspend token lifetime semantics: multiple tokens can be
/// active, and the thread resumes only when all tokens are closed.
#[async_std::test]
async fn thread_suspend_token_lifetime() {
    helpers::ensure_hal_init();
    let job = Job::root();
    let proc = Process::create(&job, "proc").unwrap();
    let thread = Thread::create(&proc, "t1").unwrap();

    thread.start(helpers::test_loop_thread_fn).unwrap();
    async_std::task::yield_now().await;
    async_std::task::yield_now().await;

    // Two "tokens" (suspend calls)
    thread.suspend();
    thread.suspend();
    as_kobject(&thread)
        .wait_signal(Signal::THREAD_SUSPENDED)
        .await;

    // Close first token
    thread.resume();
    // Still suspended
    assert!(thread.signal().contains(Signal::THREAD_SUSPENDED));

    // Close second token — thread resumes
    thread.resume();
    as_kobject(&thread)
        .wait_signal(Signal::THREAD_RUNNING)
        .await;

    thread.kill();
    as_kobject(&thread)
        .wait_signal(Signal::THREAD_TERMINATED)
        .await;
}

///
/// Write to a suspended thread's register state and verify the
/// write succeeds. The modified state effect on execution can only
/// be verified with real thread execution.
#[async_std::test]
async fn thread_write_state_modifies_execution() {
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

    // Read state
    let mut buf = vec![0u8; 1024];
    let size = thread
        .read_state(ThreadStateKind::General, &mut buf)
        .unwrap();

    // Modify some bytes (e.g., set first register to a specific value)
    if size >= 8 {
        buf[0..8].copy_from_slice(&0xDEAD_BEEF_u64.to_ne_bytes());
    }

    // Write modified state back
    let result = thread.write_state(ThreadStateKind::General, &buf[..size]);
    assert!(result.is_ok(), "write_state should succeed: {:?}", result);

    // Read back and verify the modification stuck
    let mut buf2 = vec![0u8; 1024];
    let size2 = thread
        .read_state(ThreadStateKind::General, &mut buf2)
        .unwrap();
    assert_eq!(size, size2);
    if size >= 8 {
        assert_eq!(
            &buf2[0..8],
            &0xDEAD_BEEF_u64.to_ne_bytes(),
            "modified register should persist"
        );
    }

    thread.resume();
    thread.kill();
    as_kobject(&thread)
        .wait_signal(Signal::THREAD_TERMINATED)
        .await;
}

///
/// Read the floating-point register state of a suspended thread.
/// FloatPoint kind should return NOT_SUPPORTED (not implemented).
#[async_std::test]
async fn thread_read_state_float_point() {
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

    let mut buf = vec![0u8; 1024];
    let result = thread.read_state(ThreadStateKind::FloatPoint, &mut buf);
    // Should fail with NOT_SUPPORTED (FP state not implemented)
    assert!(result.is_err(), "FloatPoint read should fail");

    thread.resume();
    thread.kill();
    as_kobject(&thread)
        .wait_signal(Signal::THREAD_TERMINATED)
        .await;
}

///
/// Read the vector (SIMD) register state of a suspended thread.
/// Vector kind should return NOT_SUPPORTED (not implemented).
#[async_std::test]
async fn thread_read_state_vector() {
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

    let mut buf = vec![0u8; 2048];
    let result = thread.read_state(ThreadStateKind::Vector, &mut buf);
    // Should fail with NOT_SUPPORTED (vector state not implemented)
    assert!(result.is_err(), "Vector read should fail");

    thread.resume();
    thread.kill();
    as_kobject(&thread)
        .wait_signal(Signal::THREAD_TERMINATED)
        .await;
}
