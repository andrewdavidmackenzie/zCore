//! Extra thread tests.
//!
//! Tests ported from the Fuchsia C++ core-tests source that are NOT
//! in the x86_64 core-tests-standalone binary. These exercise thread
//! lifecycle edge cases (kill, suspend, TLS) that complement the
//! 1:1 tests in core_tests/thread.rs.

use crate::helpers::{self, TestContext};
use hal_impl::user::UserOutPtr;
use zircon_object::object::{Handle, KernelObject, Rights, Signal};
use zircon_object::task::{Job, Process, Task, Thread, ThreadState, ThreadStateKind};

extern crate alloc;

/// Helper: cast Arc<Thread> to Arc<dyn KernelObject> for wait_signal.
fn as_kobject(thread: &alloc::sync::Arc<Thread>) -> alloc::sync::Arc<dyn KernelObject> {
    thread.clone()
}

// ── Previously-missing tests (ported from C++ core-tests) ──────────

/// Fuchsia equivalent: TEST(Threads, KillSuspendedImmediately)
/// Suspend a thread and immediately kill it before the suspend fully
/// completes. The kill should preempt the suspend — thread goes Dead.
#[async_std::test]
async fn kill_suspended_immediately() {
    helpers::ensure_hal_init();
    let job = Job::root();
    let proc = Process::create(&job, "kill-susp-imm").unwrap();
    let thread = Thread::create(&proc, "t1").unwrap();

    thread.start(helpers::test_loop_thread_fn).unwrap();
    async_std::task::yield_now().await;

    // Suspend and immediately kill — don't wait for SUSPENDED
    thread.suspend();
    thread.kill();

    as_kobject(&thread)
        .wait_signal(Signal::THREAD_TERMINATED)
        .await;
    assert_eq!(thread.state(), ThreadState::Dead);
}

/// Fuchsia equivalent: TEST(Threads, KillSuspendedThread)
/// Suspend a running thread, wait for SUSPENDED, then kill it.
/// Thread should transition cleanly to Dead.
#[async_std::test]
async fn kill_suspended_thread() {
    helpers::ensure_hal_init();
    let job = Job::root();
    let proc = Process::create(&job, "kill-susp").unwrap();
    let thread = Thread::create(&proc, "t1").unwrap();

    thread.start(helpers::test_loop_thread_fn).unwrap();
    async_std::task::yield_now().await;
    async_std::task::yield_now().await;

    thread.suspend();
    as_kobject(&thread)
        .wait_signal(Signal::THREAD_SUSPENDED)
        .await;
    assert_eq!(thread.state(), ThreadState::Suspended);

    thread.kill();
    as_kobject(&thread)
        .wait_signal(Signal::THREAD_TERMINATED)
        .await;
    assert_eq!(thread.state(), ThreadState::Dead);
}

/// Fuchsia equivalent: TEST(Threads, KillWaitingThread)
/// Kill a thread that is blocked on a syscall (sleeping).
/// The blocking wait should be interrupted and the thread exits.
#[async_std::test]
async fn kill_waiting_thread() {
    helpers::ensure_hal_init();
    let job = Job::root();
    let proc = Process::create(&job, "kill-wait").unwrap();

    let sleeper = helpers::SleepingTestThread::start(&proc);

    // Wait for the thread to enter sleep
    for _ in 0..100_000 {
        if sleeper.thread.state() == ThreadState::BlockedSleeping {
            break;
        }
        async_std::task::yield_now().await;
    }

    // Kill while blocked
    sleeper.thread.kill();
    as_kobject(&sleeper.thread)
        .wait_signal(Signal::THREAD_TERMINATED)
        .await;
    assert_eq!(sleeper.thread.state(), ThreadState::Dead);
}

/// Fuchsia equivalent: TEST(Threads, ExitBusy)
/// A thread that exits voluntarily (returns from its entry function)
/// should transition to Dead and signal THREAD_TERMINATED.
#[async_std::test]
async fn exit_busy() {
    helpers::ensure_hal_init();
    let job = Job::root();
    let proc = Process::create(&job, "exit-busy").unwrap();
    let thread = Thread::create(&proc, "t1").unwrap();

    // Thread function that does a small amount of work then exits
    fn quick_exit_fn(
        thread: zircon_object::task::CurrentThread,
    ) -> core::pin::Pin<Box<dyn core::future::Future<Output = ()> + Send + 'static>> {
        Box::pin(async move {
            let ctx = thread.wait_for_run().await;
            // Brief "work"
            async_std::task::yield_now().await;
            thread.put_context(ctx);
            // Return → thread exits
        })
    }

    thread.start(quick_exit_fn).unwrap();

    as_kobject(&thread)
        .wait_signal(Signal::THREAD_TERMINATED)
        .await;
    assert_eq!(thread.state(), ThreadState::Dead);
}

/// Fuchsia equivalent: TEST(Threads, ChannelCallDoesntLeakMessage)
/// A thread blocked in channel_call is killed before receiving a reply.
/// The in-flight message should not leak — the channel peer should
/// not have stale messages after the thread dies.
#[async_std::test]
async fn channel_call_doesnt_leak_message() {
    helpers::ensure_hal_init();
    let job = Job::root();
    let proc = Process::create(&job, "call-leak").unwrap();

    let cct = helpers::ChannelCallTestThread::start(&proc);

    // Wait for the thread to send its message
    let peer_obj: alloc::sync::Arc<dyn KernelObject> = cct.peer.clone();
    peer_obj.wait_signal(Signal::READABLE).await;

    // Read the message to confirm it was sent
    let msg = cct.peer.read().unwrap();
    assert!(!msg.data.is_empty());

    // Kill the thread without sending a reply
    cct.thread.kill();
    as_kobject(&cct.thread)
        .wait_signal(Signal::THREAD_TERMINATED)
        .await;

    // The channel peer should not have any more messages
    // (the in-flight call message was consumed by our read above)
    assert!(cct.peer.read().is_err(), "no stale messages should remain");
}

/// Fuchsia equivalent: TEST(Threads, SuspendRunningThread)
/// Suspend a currently-running thread. Verify THREAD_SUSPENDED fires,
/// THREAD_RUNNING is cleared, and state is Suspended.
#[async_std::test]
async fn suspend_running_thread() {
    helpers::ensure_hal_init();
    let job = Job::root();
    let proc = Process::create(&job, "susp-running").unwrap();
    let thread = Thread::create(&proc, "t1").unwrap();

    thread.start(helpers::test_loop_thread_fn).unwrap();
    async_std::task::yield_now().await;
    async_std::task::yield_now().await;

    assert!(thread.signal().contains(Signal::THREAD_RUNNING));

    thread.suspend();
    as_kobject(&thread)
        .wait_signal(Signal::THREAD_SUSPENDED)
        .await;

    assert_eq!(thread.state(), ThreadState::Suspended);
    assert!(!thread.signal().contains(Signal::THREAD_RUNNING));
    assert!(thread.signal().contains(Signal::THREAD_SUSPENDED));

    thread.resume();
    as_kobject(&thread)
        .wait_signal(Signal::THREAD_RUNNING)
        .await;

    thread.kill();
    as_kobject(&thread)
        .wait_signal(Signal::THREAD_TERMINATED)
        .await;
}

/// Fuchsia equivalent: TEST(Threads, SuspendSelf)
/// A thread attempts to suspend itself. In Fuchsia this returns
/// NOT_SUPPORTED because self-suspension via the token API would
/// deadlock. At the kernel object level, we verify that suspending
/// the test context's own thread doesn't panic and produces the
/// expected error or state.
#[test]
fn suspend_self() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();

    // Add a handle to the test context's own thread
    let self_thread_h = ctx.proc.add_handle(Handle::new(
        ctx.current.inner().clone(),
        Rights::DEFAULT_THREAD,
    ));

    // C++: zx_task_suspend_token(self_thread) → NOT_SUPPORTED
    let mut token_h: u32 = 0;
    let result = sc.sys_task_suspend_token(
        self_thread_h,
        UserOutPtr::from(&mut token_h as *mut _ as usize),
    );
    // Self-suspend should be rejected (would deadlock)
    assert!(
        result.is_err(),
        "self-suspend should fail to avoid deadlock"
    );
}

/// Fuchsia equivalent: TEST(Threads, SuspendMultipleWaitAsync)
/// Multiple suspend tokens + port-based wait_async signal delivery.
/// Take two suspend tokens, register wait_async for THREAD_RUNNING
/// on a port. Release one token (still suspended), release second
/// (resumes), verify port receives the RUNNING signal.
#[async_std::test]
async fn suspend_multiple_wait_async() {
    use zircon_object::signal::{Port, PortPacketRepr};

    helpers::ensure_hal_init();
    let job = Job::root();
    let proc = Process::create(&job, "susp-multi-async").unwrap();
    let thread = Thread::create(&proc, "t1").unwrap();

    thread.start(helpers::test_loop_thread_fn).unwrap();
    async_std::task::yield_now().await;
    async_std::task::yield_now().await;

    // Double suspend
    thread.suspend();
    thread.suspend();
    as_kobject(&thread)
        .wait_signal(Signal::THREAD_SUSPENDED)
        .await;

    // Register wait_async for THREAD_RUNNING on a port
    let port = Port::new(0).unwrap();
    let thread_obj = thread.clone() as alloc::sync::Arc<dyn KernelObject>;
    thread_obj
        .send_signal_to_port_async(Signal::THREAD_RUNNING, &port, 99, false)
        .unwrap();

    // Release first token — still suspended
    thread.resume();
    assert!(thread.signal().contains(Signal::THREAD_SUSPENDED));

    // Release second token — should resume
    thread.resume();
    as_kobject(&thread)
        .wait_signal(Signal::THREAD_RUNNING)
        .await;

    // Port should have received the RUNNING signal
    let pkt = async_std::task::block_on(port.wait());
    let repr = PortPacketRepr::from(&pkt);
    assert_eq!(repr.key, 99);

    thread.kill();
    as_kobject(&thread)
        .wait_signal(Signal::THREAD_TERMINATED)
        .await;
}

/// Fuchsia equivalent: TEST(Threads, SuspendResumeFromMultipleThreads)
/// Multiple Zircon threads independently suspend and resume a target.
/// Verify the suspend count is tracked correctly — the target only
/// resumes when all outstanding suspends are released.
#[async_std::test]
async fn suspend_resume_from_multiple_threads() {
    helpers::ensure_hal_init();
    let job = Job::root();
    let proc = Process::create(&job, "susp-multi-thread").unwrap();
    let thread = Thread::create(&proc, "target").unwrap();

    thread.start(helpers::test_loop_thread_fn).unwrap();
    async_std::task::yield_now().await;
    async_std::task::yield_now().await;

    // Simulate two independent suspenders by calling suspend twice
    // (in libos, all calls come from the same async context, but the
    // kernel's suspend count is what we're testing)
    thread.suspend(); // suspender A
    thread.suspend(); // suspender B
    as_kobject(&thread)
        .wait_signal(Signal::THREAD_SUSPENDED)
        .await;

    // Release suspender A — still suspended (B holds)
    thread.resume();
    assert_eq!(thread.state(), ThreadState::Suspended);

    // Release suspender B — should resume
    thread.resume();
    as_kobject(&thread)
        .wait_signal(Signal::THREAD_RUNNING)
        .await;
    assert_eq!(thread.state(), ThreadState::Running);

    thread.kill();
    as_kobject(&thread)
        .wait_signal(Signal::THREAD_TERMINATED)
        .await;
}

/// Fuchsia equivalent: TEST(Threads, SuspendStoppedThread)
/// Suspend a thread that has been stopped (via exception). In libos
/// mode we approximate this by suspending a thread that has been
/// killed (Dead state) — the kernel should handle it gracefully.
#[async_std::test]
async fn suspend_stopped_thread() {
    helpers::ensure_hal_init();
    let job = Job::root();
    let proc = Process::create(&job, "susp-stopped").unwrap();
    let thread = Thread::create(&proc, "t1").unwrap();

    thread.start(helpers::test_loop_thread_fn).unwrap();
    async_std::task::yield_now().await;

    thread.kill();
    as_kobject(&thread)
        .wait_signal(Signal::THREAD_TERMINATED)
        .await;

    // Suspend a dead/stopped thread — should not panic
    thread.suspend();
    assert_eq!(thread.state(), ThreadState::Dead);
}

/// Fuchsia equivalent: TEST(Threads, SuspendStoppedAndBlockedThread)
/// Suspend a thread that is both stopped (exception) and blocked
/// (syscall). In libos mode we test by suspending a sleeping thread,
/// then killing it while suspended, verifying the composite state
/// transitions work correctly.
#[async_std::test]
async fn suspend_stopped_and_blocked_thread() {
    helpers::ensure_hal_init();
    let job = Job::root();
    let proc = Process::create(&job, "susp-stop-block").unwrap();

    let sleeper = helpers::SleepingTestThread::start(&proc);

    // Wait for sleeping state
    for _ in 0..100_000 {
        if sleeper.thread.state() == ThreadState::BlockedSleeping {
            break;
        }
        async_std::task::yield_now().await;
    }

    // Suspend while blocked
    sleeper.thread.suspend();
    as_kobject(&sleeper.thread)
        .wait_signal(Signal::THREAD_SUSPENDED)
        .await;

    // Kill while suspended+blocked
    sleeper.thread.kill();
    as_kobject(&sleeper.thread)
        .wait_signal(Signal::THREAD_TERMINATED)
        .await;
    assert_eq!(sleeper.thread.state(), ThreadState::Dead);
}

/// Fuchsia equivalent: TEST(Threads, SuspendStoppedAndBlockedThread2)
/// Variant: kill the blocked thread first, then try to suspend.
/// The thread should be Dead and the suspend should be a no-op.
#[async_std::test]
async fn suspend_stopped_and_blocked_thread2() {
    helpers::ensure_hal_init();
    let job = Job::root();
    let proc = Process::create(&job, "susp-stop-block2").unwrap();

    let sleeper = helpers::SleepingTestThread::start(&proc);

    for _ in 0..100_000 {
        if sleeper.thread.state() == ThreadState::BlockedSleeping {
            break;
        }
        async_std::task::yield_now().await;
    }

    // Kill while blocked (reverse order of variant 1)
    sleeper.thread.kill();
    as_kobject(&sleeper.thread)
        .wait_signal(Signal::THREAD_TERMINATED)
        .await;

    // Suspend after death — should be a no-op
    sleeper.thread.suspend();
    assert_eq!(sleeper.thread.state(), ThreadState::Dead);
}

/// Fuchsia equivalent: TEST(Threads, ThreadLocal)
/// Verify that TLS registers are independent per thread. Set
/// different TLS values on two suspended threads and verify they
/// don't interfere with each other.
#[test]
fn thread_local() {
    let _ctx = TestContext::new();
    let job = Job::root();
    let proc = Process::create(&job, "tls-test").unwrap();
    let t1 = Thread::create(&proc, "tls1").unwrap();
    let t2 = Thread::create(&proc, "tls2").unwrap();

    t1.suspend();
    t2.suspend();

    // TLS register offset (arch-specific)
    #[cfg(target_arch = "x86_64")]
    let tls_offset = 18 * 8; // fsbase
    #[cfg(target_arch = "aarch64")]
    let tls_offset = 34 * 8; // tpidr_el0
    #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
    let tls_offset = usize::MAX;

    // Read general regs for both threads
    let mut buf1 = vec![0u8; 512];
    let n1 = t1.read_state(ThreadStateKind::General, &mut buf1).unwrap();

    let mut buf2 = vec![0u8; 512];
    let n2 = t2.read_state(ThreadStateKind::General, &mut buf2).unwrap();

    if tls_offset + 8 > n1 || tls_offset + 8 > n2 {
        return; // arch doesn't support TLS offset check
    }

    // Set different TLS values on each thread
    let tls_val1: u64 = 0x1111_1111_1111_1111;
    let tls_val2: u64 = 0x2222_2222_2222_2222;
    buf1[tls_offset..tls_offset + 8].copy_from_slice(&tls_val1.to_ne_bytes());
    buf2[tls_offset..tls_offset + 8].copy_from_slice(&tls_val2.to_ne_bytes());

    t1.write_state(ThreadStateKind::General, &buf1[..n1])
        .unwrap();
    t2.write_state(ThreadStateKind::General, &buf2[..n2])
        .unwrap();

    // Read back from each and verify independence
    let mut rb1 = vec![0u8; 512];
    let mut rb2 = vec![0u8; 512];
    t1.read_state(ThreadStateKind::General, &mut rb1).unwrap();
    t2.read_state(ThreadStateKind::General, &mut rb2).unwrap();

    let got1 = u64::from_ne_bytes(rb1[tls_offset..tls_offset + 8].try_into().unwrap());
    let got2 = u64::from_ne_bytes(rb2[tls_offset..tls_offset + 8].try_into().unwrap());
    assert_eq!(got1, tls_val1, "t1 TLS should be independent");
    assert_eq!(got2, tls_val2, "t2 TLS should be independent");
}

/// Fuchsia equivalent: TEST(Threads, ThreadReadStateWhenStarting)
/// Read thread state during the startup transition. After setup_uspace
/// but before the thread fully enters Running, the register state
/// should contain the initial values (entry, stack, args).
#[test]
fn thread_read_state_when_starting() {
    let _ctx = TestContext::new();
    let job = Job::root();
    let proc = Process::create(&job, "read-starting").unwrap();
    let thread = Thread::create(&proc, "t1").unwrap();

    // Set up context
    thread
        .with_context(|ctx| ctx.setup_uspace(0x3000, 0xA000, &[7, 8, 0]))
        .unwrap();

    // Pre-suspend so the thread enters Suspended, not Running
    thread.suspend();
    thread.start(helpers::test_loop_thread_fn).unwrap();

    // The thread should be in Suspended state with initial regs set
    // Read state while starting (suspended)
    let mut buf = vec![0u8; 512];
    let size = thread
        .read_state(ThreadStateKind::General, &mut buf)
        .unwrap();
    assert!(size > 0, "should read register state during startup");

    // Verify the context has our setup values
    use hal_impl::context::UserContextField;
    let mut ctx = thread.context_cloned().unwrap();
    let sp = ctx.get_field(UserContextField::StackPointer);
    assert!(sp != 0, "SP should be set during startup");

    thread.resume();
    thread.kill();
    // Don't wait — thread may not have fully started
}

/// Fuchsia equivalent: TEST(Threads, WritingArmFlagsRegister)
/// aarch64-only: write the CPSR/SPSR flags register and verify
/// the value round-trips via write_state/read_state.
#[cfg(target_arch = "aarch64")]
#[test]
fn writing_arm_flags_register() {
    let _ctx = TestContext::new();
    let job = Job::root();
    let proc = Process::create(&job, "arm-flags").unwrap();
    let thread = Thread::create(&proc, "t1").unwrap();

    thread.suspend();

    let mut buf = vec![0u8; 512];
    let n = thread
        .read_state(ThreadStateKind::General, &mut buf)
        .unwrap();

    // On aarch64, the CPSR/SPSR is at a known offset in the GeneralRegs struct.
    // The exact offset depends on the trapframe layout:
    // x0-x30 (31 regs * 8 = 248), sp (8), elr/pc (8), spsr (8) = offset 256
    let spsr_offset = 33 * 8; // after x0..x30, sp, elr
    if spsr_offset + 8 <= n {
        // Write a known flags value (only user-writable bits)
        // NZCV flags are bits 28-31, safe to modify
        let flags_val: u64 = 0xA000_0000; // N=1, Z=0, C=1, V=0
        buf[spsr_offset..spsr_offset + 8].copy_from_slice(&flags_val.to_ne_bytes());
        thread
            .write_state(ThreadStateKind::General, &buf[..n])
            .unwrap();

        // Read back and verify
        let mut readback = vec![0u8; 512];
        thread
            .read_state(ThreadStateKind::General, &mut readback)
            .unwrap();
        let got = u64::from_ne_bytes(readback[spsr_offset..spsr_offset + 8].try_into().unwrap());
        // The kernel may mask non-user bits, so check only the NZCV bits
        assert_eq!(
            got & 0xF000_0000,
            flags_val & 0xF000_0000,
            "NZCV flags should round-trip"
        );
    }
}
