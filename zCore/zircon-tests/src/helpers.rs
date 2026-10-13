//! Test helper: creates a minimal Zircon process/thread context
//! for dispatching syscalls in libos mode.

#![allow(dead_code)]

use alloc::boxed::Box;
use alloc::sync::Arc;
use core::future::Future;
use core::pin::Pin;
use zircon_object::dev::{Resource, ResourceFlags, ResourceKind};
use zircon_object::object::{Handle, Rights};
use zircon_object::task::{CurrentThread, Job, Process, Task, Thread};

extern crate alloc;

type HandleValue = u32;

/// One-time HAL initialization for libos mode.
static INIT: std::sync::Once = std::sync::Once::new();

pub fn ensure_hal_init() {
    INIT.call_once(|| {
        hal_impl::init();
    });
}

/// Thread function placeholder for the Syscall struct.
/// In test mode, we never actually spawn user threads through this.
fn dummy_thread_fn(_thread: CurrentThread) -> Pin<Box<dyn Future<Output = ()> + Send + 'static>> {
    Box::pin(async {})
}

/// Thread function that simulates a real userspace execution loop.
///
/// Like the real `run_user()` in zircon-loader, this loops:
/// 1. `wait_for_run()` — takes context (blocks if suspended)
/// 2. yield — gives the test harness a chance to act
/// 3. `put_context()` — returns context (enables read/write_state)
///
/// The thread exits when killed (state becomes Dying).
pub fn test_loop_thread_fn(
    thread: CurrentThread,
) -> Pin<Box<dyn Future<Output = ()> + Send + 'static>> {
    Box::pin(async move {
        loop {
            let ctx = thread.wait_for_run().await;
            // Check if the thread was killed while we were waiting.
            if thread.state() == zircon_object::task::ThreadState::Dying {
                thread.put_context(ctx);
                break;
            }
            // Simulate brief "execution" then yield back to the scheduler.
            async_std::task::yield_now().await;
            thread.put_context(ctx);
        }
        // CurrentThread drop calls terminate() → state becomes Dead,
        // THREAD_TERMINATED signal is asserted.
    })
}

// ── Concurrent futex test thread ─────────────────────────────────────
//
// Rust equivalent of the C++ TestThread class in futex.cc.
// Creates a real Zircon thread that calls futex.wait(), just like the
// C++ TestThread creates a real Zircon thread via thrd_create that
// calls zx_futex_wait().

use core::sync::atomic::{AtomicU32, Ordering};
use zircon_object::signal::Futex;

/// Shared state between the test and its waiter thread.
///
/// Mirrors the C++ TestThread's atomic state machine:
/// kWaitingToStart(100) → kAboutToWait(200) → kWaitReturned(300)
pub struct FutexWaiterState {
    /// The futex to wait on.
    pub futex: Arc<Futex>,
    /// The expected value for futex_wait.
    pub expected_value: i32,
    /// Thread state: 100=waiting to start, 200=about to wait, 300=wait returned
    pub state: AtomicU32,
    /// Result of the wait: 0=Ok, nonzero=error code
    pub wait_result: AtomicU32,
}

const WAITER_STATE_WAITING_TO_START: u32 = 100;
const WAITER_STATE_ABOUT_TO_WAIT: u32 = 200;
const WAITER_STATE_WAIT_RETURNED: u32 = 300;

impl FutexWaiterState {
    pub fn new(futex: Arc<Futex>, expected_value: i32) -> Arc<Self> {
        Arc::new(Self {
            futex,
            expected_value,
            state: AtomicU32::new(WAITER_STATE_WAITING_TO_START),
            wait_result: AtomicU32::new(u32::MAX),
        })
    }

    pub fn has_wait_returned(&self) -> bool {
        self.state.load(Ordering::Acquire) == WAITER_STATE_WAIT_RETURNED
    }
}

/// Thread function for futex waiter threads.
///
/// C++ equivalent: TestThread::Run()
///   state_.store(State::kAboutToWait);
///   wait_result_.store(zx_futex_wait(futex(), *futex(), owner, deadline));
///   state_.store(State::kWaitReturned);
///
/// The thread reads its FutexWaiterState from the Thread ext.
/// Uses `blocking_run` so the wait is cancellable via Thread::kill(),
/// matching the real zx_futex_wait syscall path.
pub fn futex_waiter_thread_fn(
    thread: CurrentThread,
) -> Pin<Box<dyn Future<Output = ()> + Send + 'static>> {
    Box::pin(async move {
        let inner_thread = thread.inner();
        let waiter_state = inner_thread
            .ext()
            .downcast_ref::<Arc<FutexWaiterState>>()
            .expect("FutexWaiterState ext missing")
            .clone();

        // C++: state_.store(State::kAboutToWait);
        waiter_state
            .state
            .store(WAITER_STATE_ABOUT_TO_WAIT, Ordering::Release);

        // C++: wait_result_.store(zx_futex_wait(futex(), *futex(), owner, deadline));
        // Use blocking_run so the wait is cancellable via Thread::kill().
        let wait_future = core::pin::pin!(waiter_state.futex.wait(waiter_state.expected_value));
        let result = thread
            .blocking_run(
                wait_future,
                zircon_object::task::ThreadState::BlockedFutex,
                core::time::Duration::from_secs(3600), // effectively infinite
                None,
            )
            .await;

        // Store result: 0 for Ok, error code for Err
        let code = match result {
            Ok(()) => 0,
            Err(e) => e as u32,
        };
        waiter_state.wait_result.store(code, Ordering::Release);

        // C++: state_.store(State::kWaitReturned);
        waiter_state
            .state
            .store(WAITER_STATE_WAIT_RETURNED, Ordering::Release);

        // CurrentThread drop calls terminate()
    })
}

/// A handle to a running futex waiter thread.
///
/// Rust equivalent of the C++ TestThread class. Owns the Thread Arc
/// and the shared FutexWaiterState.
pub struct FutexTestThread {
    pub thread: Arc<Thread>,
    pub state: Arc<FutexWaiterState>,
}

impl FutexTestThread {
    /// Create and start a waiter thread on the given futex.
    ///
    /// C++ equivalent: TestThread::Start(&futex)
    ///
    /// Creates a Zircon Thread (via Thread::create_with_ext), attaches
    /// the FutexWaiterState as ext, and starts it with
    /// futex_waiter_thread_fn.
    pub fn start(proc: &Arc<Process>, futex: &Arc<Futex>, expected_value: i32) -> Self {
        let waiter_state = FutexWaiterState::new(futex.clone(), expected_value);
        let thread = Thread::create_with_ext(proc, "futex-waiter", waiter_state.clone()).unwrap();
        thread.start(futex_waiter_thread_fn).unwrap();
        FutexTestThread {
            thread,
            state: waiter_state,
        }
    }

    /// Check if the thread's wait has returned.
    /// C++ equivalent: TestThread::HasWaitReturned()
    pub fn has_wait_returned(&self) -> bool {
        self.state.has_wait_returned()
    }

    /// Get the wait result (0 = Ok, nonzero = ZxError code).
    /// C++ equivalent: TestThread::wait_result()
    pub fn wait_result(&self) -> u32 {
        self.state.wait_result.load(Ordering::Acquire)
    }

    /// Wait until the thread's futex_wait has returned.
    /// C++ equivalent: TestThread::WaitUntilWoken()
    ///
    /// Polls in a loop, yielding to the scheduler. Panics if the
    /// thread does not wake within a reasonable time.
    pub async fn wait_until_woken(&self) {
        let deadline = std::time::Instant::now() + core::time::Duration::from_secs(10);
        while !self.has_wait_returned() {
            assert!(
                std::time::Instant::now() < deadline,
                "FutexTestThread: timed out waiting for thread to wake"
            );
            // Yield to let the waiter thread make progress
            async_std::task::yield_now().await;
        }
    }

    /// Wait for the thread to terminate.
    /// C++ equivalent: TestThread::Shutdown()
    ///
    /// If the thread hasn't returned from its wait, kills it first.
    /// Then waits for THREAD_TERMINATED with a timeout.
    pub async fn shutdown(&self) {
        if !self.has_wait_returned() {
            // Kill the thread — this sends the killed signal which
            // causes blocking_run's select to return Err(STOP).
            self.thread.kill();
        }
        // Give the thread time to finish its cleanup and terminate.
        let deadline = std::time::Instant::now() + core::time::Duration::from_secs(5);
        loop {
            if self.thread.state() == zircon_object::task::ThreadState::Dead {
                break;
            }
            if std::time::Instant::now() >= deadline {
                // Thread did not terminate — force it.
                // This can happen if the async runtime hasn't scheduled
                // the thread's task yet.
                break;
            }
            async_std::task::yield_now().await;
        }
    }
}

/// Count how many threads in a slice have had their wait return.
/// C++ equivalent: CountWoken lambda in AssertWokeThreadCount
pub fn count_woken(threads: &[FutexTestThread]) -> usize {
    threads.iter().filter(|t| t.has_wait_returned()).count()
}

/// Wait until exactly `target` threads have woken, then verify
/// no additional threads wake after a settling delay.
///
/// C++ equivalent: AssertWokeThreadCount()
pub async fn assert_woke_thread_count(threads: &[FutexTestThread], target: usize) {
    assert!(target <= threads.len());

    // Wait until the target count is reached
    let deadline = std::time::Instant::now() + core::time::Duration::from_secs(10);
    while count_woken(threads) < target {
        assert!(
            std::time::Instant::now() < deadline,
            "timed out waiting for {} threads to wake (currently {})",
            target,
            count_woken(threads)
        );
        async_std::task::yield_now().await;
    }
    assert_eq!(count_woken(threads), target);

    // C++: zx::nanosleep(zx::deadline_after(zx::msec(300)));
    // Wait to verify no spurious wakeups
    async_std::task::sleep(core::time::Duration::from_millis(100)).await;
    assert_eq!(
        count_woken(threads),
        target,
        "spurious wakeup: expected {} woken but found {}",
        target,
        count_woken(threads)
    );
}

/// A test context providing a Zircon process and current thread.
pub struct TestContext {
    pub proc: Arc<Process>,
    pub current: CurrentThread,
}

impl TestContext {
    /// Create a minimal Zircon process/thread context for testing.
    pub fn new() -> Self {
        ensure_hal_init();
        let job = Job::root();
        let proc = Process::create(&job, "test-proc").unwrap();
        let thread = Thread::create(&proc, "test-thread").unwrap();
        let current = CurrentThread::new_for_test(thread);
        TestContext { proc, current }
    }

    /// Construct a Syscall dispatcher for this context.
    pub fn syscall(&self) -> zircon_syscall::Syscall<'_> {
        zircon_syscall::Syscall {
            thread: &self.current,
            thread_fn: dummy_thread_fn,
        }
    }

    /// Create a root resource and add it to the process handle table.
    ///
    /// Returns the handle value. The root resource validates as any
    /// resource kind and bypasses range checks, matching the behavior
    /// of the root resource created during Zircon boot.
    pub fn add_root_resource(&self) -> HandleValue {
        let resource = Resource::create(
            "root",
            ResourceKind::ROOT,
            0,
            0x1_0000_0000, // 4 GiB range, same as boot path
            ResourceFlags::empty(),
        );
        self.proc
            .add_handle(Handle::new(resource, Rights::DEFAULT_RESOURCE))
    }
}
