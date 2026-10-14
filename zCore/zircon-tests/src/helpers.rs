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

pub const WAITER_STATE_WAITING_TO_START: u32 = 100;
pub const WAITER_STATE_ABOUT_TO_WAIT: u32 = 200;
pub const WAITER_STATE_WAIT_RETURNED: u32 = 300;

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
        // Pass the thread reference so the kernel tracks which thread is
        // waiting — needed for is_valid_new_owner checks in futex_wait
        // and futex_requeue with owner validation.
        let wait_future = core::pin::pin!(waiter_state.futex.wait_with_owner(
            waiter_state.expected_value,
            Some(inner_thread.clone()),
            None,
        ));
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

// ── Sync completion helper ───────────────────────────────────────────
//
// Rust equivalent of Fuchsia's sync_completion_t from libsync.
// Implements the same 3-state protocol:
//   UNSIGNALED(0) → UNSIGNALED_WITH_WAITERS(1) → SIGNALED(2)

const SYNC_UNSIGNALED: i32 = 0;
const SYNC_UNSIGNALED_WITH_WAITERS: i32 = 1;
const SYNC_SIGNALED: i32 = 2;

/// A sync_completion_t equivalent built on Futex + AtomicI32.
///
/// This replicates the exact same futex protocol as Fuchsia's
/// sync_completion in libsync, so the tests exercise the same
/// kernel futex_wait/futex_wake code paths.
pub struct SyncCompletion {
    pub value: std::sync::Arc<core::sync::atomic::AtomicI32>,
    pub futex: Arc<Futex>,
}

impl SyncCompletion {
    pub fn new() -> Self {
        let value = std::sync::Arc::new(core::sync::atomic::AtomicI32::new(SYNC_UNSIGNALED));
        let futex = Futex::new(
            std::sync::Arc::as_ptr(&value) as *const core::sync::atomic::AtomicI32 as usize
        );
        SyncCompletion { value, futex }
    }

    /// C equivalent: sync_completion_signal()
    pub fn signal(&self) {
        use core::sync::atomic::AtomicI32;
        let _futex_ptr = &*self.value as *const AtomicI32;
        let prev = self.value.swap(SYNC_SIGNALED, Ordering::SeqCst);
        if prev == SYNC_UNSIGNALED_WITH_WAITERS {
            self.futex.wake(usize::MAX);
        }
    }

    /// C equivalent: sync_completion_reset()
    pub fn reset(&self) {
        let _ = self.value.compare_exchange(
            SYNC_SIGNALED,
            SYNC_UNSIGNALED,
            Ordering::SeqCst,
            Ordering::Acquire,
        );
    }

    /// C equivalent: sync_completion_signaled()
    pub fn signaled(&self) -> bool {
        self.value.load(Ordering::Acquire) == SYNC_SIGNALED
    }

    /// C equivalent: sync_completion_signal_requeue()
    pub fn signal_requeue(&self, target_futex: &Arc<Futex>) {
        self.value.store(SYNC_SIGNALED, Ordering::SeqCst);
        // Requeue all waiters to the target futex (wake 0, requeue all)
        let _ = self
            .futex
            .requeue(SYNC_SIGNALED, 0, usize::MAX, target_futex, None, true);
    }
}

/// Shared state for a sync_completion waiter thread.
pub struct CompletionWaiterState {
    pub completion: std::sync::Arc<SyncCompletion>,
    pub state: AtomicU32,
    pub wait_result: AtomicU32,
}

impl CompletionWaiterState {
    pub fn new(completion: std::sync::Arc<SyncCompletion>) -> Arc<Self> {
        Arc::new(Self {
            completion,
            state: AtomicU32::new(WAITER_STATE_WAITING_TO_START),
            wait_result: AtomicU32::new(u32::MAX),
        })
    }

    pub fn has_wait_returned(&self) -> bool {
        self.state.load(Ordering::Acquire) == WAITER_STATE_WAIT_RETURNED
    }
}

/// Thread function for sync_completion waiter threads.
///
/// Implements the same protocol as sync_completion_wait_deadline:
/// 1. CAS(UNSIGNALED → UNSIGNALED_WITH_WAITERS)
/// 2. If was SIGNALED, return immediately
/// 3. futex_wait(&futex, UNSIGNALED_WITH_WAITERS)
/// 4. On wake, check if SIGNALED (loop if not)
pub fn completion_waiter_thread_fn(
    thread: CurrentThread,
) -> Pin<Box<dyn Future<Output = ()> + Send + 'static>> {
    Box::pin(async move {
        let inner_thread = thread.inner();
        let waiter_state = inner_thread
            .ext()
            .downcast_ref::<Arc<CompletionWaiterState>>()
            .expect("CompletionWaiterState ext missing")
            .clone();

        waiter_state
            .state
            .store(WAITER_STATE_ABOUT_TO_WAIT, Ordering::Release);

        let completion = &waiter_state.completion;

        // C: atomic_compare_exchange_strong(futex, &prev_value, UNSIGNALED_WITH_WAITERS)
        let mut prev = SYNC_UNSIGNALED;
        let _ = completion.value.compare_exchange(
            prev,
            SYNC_UNSIGNALED_WITH_WAITERS,
            Ordering::SeqCst,
            Ordering::Acquire,
        );
        prev = completion.value.load(Ordering::Acquire);

        let result = if prev == SYNC_SIGNALED {
            // Already signaled — return immediately
            Ok(())
        } else {
            // Wait loop — handles spurious wakeups
            loop {
                let wait_future = core::pin::pin!(completion.futex.wait_with_owner(
                    SYNC_UNSIGNALED_WITH_WAITERS,
                    Some(inner_thread.clone()),
                    None,
                ));
                let wait_result = thread
                    .blocking_run(
                        wait_future,
                        zircon_object::task::ThreadState::BlockedFutex,
                        core::time::Duration::from_secs(3600),
                        None,
                    )
                    .await;

                match wait_result {
                    Ok(()) => {
                        // Woken — check if actually signaled
                        if completion.value.load(Ordering::Acquire) != SYNC_UNSIGNALED_WITH_WAITERS
                        {
                            break Ok(());
                        }
                        // Spurious wake — go back to sleep
                    }
                    Err(zircon_object::ZxError::BAD_STATE) => {
                        // Value changed (now SIGNALED) — success
                        break Ok(());
                    }
                    Err(e) => break Err(e),
                }
            }
        };

        let code = match result {
            Ok(()) => 0,
            Err(e) => e as u32,
        };
        waiter_state.wait_result.store(code, Ordering::Release);
        waiter_state
            .state
            .store(WAITER_STATE_WAIT_RETURNED, Ordering::Release);
    })
}

/// A handle to a running sync_completion waiter thread.
pub struct CompletionTestThread {
    pub thread: Arc<Thread>,
    pub state: Arc<CompletionWaiterState>,
}

impl CompletionTestThread {
    pub fn start(proc: &Arc<Process>, completion: &std::sync::Arc<SyncCompletion>) -> Self {
        let waiter_state = CompletionWaiterState::new(completion.clone());
        let thread =
            Thread::create_with_ext(proc, "completion-waiter", waiter_state.clone()).unwrap();
        thread.start(completion_waiter_thread_fn).unwrap();
        CompletionTestThread {
            thread,
            state: waiter_state,
        }
    }

    pub fn has_wait_returned(&self) -> bool {
        self.state.has_wait_returned()
    }

    pub fn wait_result(&self) -> u32 {
        self.state.wait_result.load(Ordering::Acquire)
    }

    pub async fn wait_until_woken(&self) {
        let deadline = std::time::Instant::now() + core::time::Duration::from_secs(10);
        while !self.has_wait_returned() {
            assert!(
                std::time::Instant::now() < deadline,
                "CompletionTestThread: timed out waiting for thread to wake"
            );
            async_std::task::yield_now().await;
        }
    }

    pub async fn shutdown(&self) {
        if !self.has_wait_returned() {
            self.thread.kill();
        }
        let deadline = std::time::Instant::now() + core::time::Duration::from_secs(5);
        loop {
            if self.thread.state() == zircon_object::task::ThreadState::Dead {
                break;
            }
            if std::time::Instant::now() >= deadline {
                break;
            }
            async_std::task::yield_now().await;
        }
    }
}

// ── Sync mutex helper ────────────────────────────────────────────────
//
// Rust equivalent of Fuchsia's sync_mutex_t from libsync.
// Uses a simple 3-state protocol:
//   UNLOCKED(0) → LOCKED_WITHOUT_WAITERS(1) → LOCKED_WITH_WAITERS(2)

const MUTEX_UNLOCKED: i32 = 0;
const MUTEX_LOCKED: i32 = 1;
const MUTEX_LOCKED_WITH_WAITERS: i32 = 2;

/// A sync_mutex_t equivalent built on Futex + AtomicI32.
pub struct SyncMutex {
    pub value: std::sync::Arc<core::sync::atomic::AtomicI32>,
    pub futex: Arc<Futex>,
}

impl SyncMutex {
    pub fn new() -> Self {
        let value = std::sync::Arc::new(core::sync::atomic::AtomicI32::new(MUTEX_UNLOCKED));
        let futex = Futex::new(
            std::sync::Arc::as_ptr(&value) as *const core::sync::atomic::AtomicI32 as usize
        );
        SyncMutex { value, futex }
    }

    /// C equivalent: sync_mutex_lock() — blocking lock
    /// Uses CAS to try to acquire; if contended, does futex_wait.
    pub async fn lock(&self) {
        // Fast path: CAS 0 → 1
        if self
            .value
            .compare_exchange(
                MUTEX_UNLOCKED,
                MUTEX_LOCKED,
                Ordering::Acquire,
                Ordering::Relaxed,
            )
            .is_ok()
        {
            return;
        }
        // Slow path: mark as contended and wait
        loop {
            // If not already LOCKED_WITH_WAITERS, set it
            if self
                .value
                .swap(MUTEX_LOCKED_WITH_WAITERS, Ordering::Acquire)
                == MUTEX_UNLOCKED
            {
                // We got the lock (it was unlocked)
                return;
            }
            // Wait until the value changes from LOCKED_WITH_WAITERS
            let _ = self.futex.wait(MUTEX_LOCKED_WITH_WAITERS).await;
        }
    }

    /// C equivalent: sync_mutex_unlock() — release lock
    pub fn unlock(&self) {
        if self.value.swap(MUTEX_UNLOCKED, Ordering::Release) == MUTEX_LOCKED_WITH_WAITERS {
            // There were waiters — wake one
            self.futex.wake(1);
        }
    }
}

// ── Sync condition variable helper ──────────────────────────────────

/// A sync_condition_t equivalent built on Futex + AtomicI32.
pub struct SyncCondition {
    pub value: std::sync::Arc<core::sync::atomic::AtomicI32>,
    pub futex: Arc<Futex>,
}

impl SyncCondition {
    pub fn new() -> Self {
        let value = std::sync::Arc::new(core::sync::atomic::AtomicI32::new(0));
        let futex = Futex::new(
            std::sync::Arc::as_ptr(&value) as *const core::sync::atomic::AtomicI32 as usize
        );
        SyncCondition { value, futex }
    }

    /// C equivalent: sync_condition_wait(cond, mutex)
    /// Unlocks mutex, waits on futex, re-locks mutex.
    pub async fn wait(&self, mutex: &SyncMutex) {
        let seq = self.value.load(Ordering::Acquire);
        mutex.unlock();
        let _ = self.futex.wait(seq).await;
        mutex.lock().await;
    }

    /// C equivalent: sync_condition_signal(cond) — wake one
    pub fn signal(&self) {
        self.value.fetch_add(1, Ordering::Release);
        self.futex.wake(1);
    }

    /// C equivalent: sync_condition_broadcast(cond) — wake all
    pub fn broadcast(&self) {
        self.value.fetch_add(1, Ordering::Release);
        self.futex.wake(usize::MAX);
    }
}

/// Shared state for mutex contention waiter threads.
pub struct MutexContenderState {
    pub mutex: std::sync::Arc<SyncMutex>,
    pub counter: std::sync::Arc<core::sync::atomic::AtomicI32>,
    pub iterations: usize,
    pub state: AtomicU32,
}

impl MutexContenderState {
    pub fn new(
        mutex: std::sync::Arc<SyncMutex>,
        counter: std::sync::Arc<core::sync::atomic::AtomicI32>,
        iterations: usize,
    ) -> Arc<Self> {
        Arc::new(Self {
            mutex,
            counter,
            iterations,
            state: AtomicU32::new(WAITER_STATE_WAITING_TO_START),
        })
    }
}

/// Thread function for mutex contention threads.
///
/// C++ equivalent: each TestThread repeatedly does mutex.lock, counter++, mutex.unlock.
pub fn mutex_contender_thread_fn(
    thread: CurrentThread,
) -> Pin<Box<dyn Future<Output = ()> + Send + 'static>> {
    Box::pin(async move {
        let inner_thread = thread.inner();
        let contender = inner_thread
            .ext()
            .downcast_ref::<Arc<MutexContenderState>>()
            .expect("MutexContenderState ext missing")
            .clone();

        contender
            .state
            .store(WAITER_STATE_ABOUT_TO_WAIT, Ordering::Release);

        for _ in 0..contender.iterations {
            contender.mutex.lock().await;
            contender.counter.fetch_add(1, Ordering::SeqCst);
            contender.mutex.unlock();
            // Small yield to let other threads contend
            async_std::task::yield_now().await;
        }

        contender
            .state
            .store(WAITER_STATE_WAIT_RETURNED, Ordering::Release);
    })
}

/// Shared state for condvar waiter threads.
pub struct CondvarWaiterState {
    pub mutex: std::sync::Arc<SyncMutex>,
    pub cond: std::sync::Arc<SyncCondition>,
    pub predicate: std::sync::Arc<core::sync::atomic::AtomicBool>,
    pub woken_counter: std::sync::Arc<core::sync::atomic::AtomicI32>,
    pub state: AtomicU32,
}

impl CondvarWaiterState {
    pub fn new(
        mutex: std::sync::Arc<SyncMutex>,
        cond: std::sync::Arc<SyncCondition>,
        predicate: std::sync::Arc<core::sync::atomic::AtomicBool>,
        woken_counter: std::sync::Arc<core::sync::atomic::AtomicI32>,
    ) -> Arc<Self> {
        Arc::new(Self {
            mutex,
            cond,
            predicate,
            woken_counter,
            state: AtomicU32::new(WAITER_STATE_WAITING_TO_START),
        })
    }
}

/// Thread function for condvar waiter threads.
///
/// C++ equivalent: mutex.lock, while (!predicate) cond.wait(&mutex),
/// woken_counter++, mutex.unlock.
pub fn condvar_waiter_thread_fn(
    thread: CurrentThread,
) -> Pin<Box<dyn Future<Output = ()> + Send + 'static>> {
    Box::pin(async move {
        let inner_thread = thread.inner();
        let waiter = inner_thread
            .ext()
            .downcast_ref::<Arc<CondvarWaiterState>>()
            .expect("CondvarWaiterState ext missing")
            .clone();

        waiter
            .state
            .store(WAITER_STATE_ABOUT_TO_WAIT, Ordering::Release);

        waiter.mutex.lock().await;
        while !waiter.predicate.load(Ordering::Acquire) {
            waiter.cond.wait(&waiter.mutex).await;
        }
        waiter.woken_counter.fetch_add(1, Ordering::SeqCst);
        waiter.mutex.unlock();

        waiter
            .state
            .store(WAITER_STATE_WAIT_RETURNED, Ordering::Release);
    })
}

// ── Echo server helper ───────────────────────────────────────────────
//
// A Zircon thread that reads messages from a channel and writes them
// back (echo). Used by channel_call and iovec tests.

use zircon_object::ipc::Channel;
use zircon_object::object::{KernelObject, Signal};

/// Shared state for an echo server thread.
pub struct EchoServerState {
    /// The server's end of the channel.
    pub channel: Arc<Channel>,
    /// Thread state tracking.
    pub state: AtomicU32,
}

impl EchoServerState {
    pub fn new(channel: Arc<Channel>) -> Arc<Self> {
        Arc::new(Self {
            channel,
            state: AtomicU32::new(WAITER_STATE_WAITING_TO_START),
        })
    }
}

/// Thread function for echo server.
///
/// C++ equivalent: EchoServer in channel_call tests.
/// Loops: wait_signal(READABLE) → read → write back.
/// Exits when the channel peer is closed.
pub fn echo_server_thread_fn(
    thread: CurrentThread,
) -> Pin<Box<dyn Future<Output = ()> + Send + 'static>> {
    Box::pin(async move {
        let inner_thread = thread.inner();
        let server = inner_thread
            .ext()
            .downcast_ref::<Arc<EchoServerState>>()
            .expect("EchoServerState ext missing")
            .clone();

        server
            .state
            .store(WAITER_STATE_ABOUT_TO_WAIT, Ordering::Release);

        let channel = &server.channel;
        let channel_obj: Arc<dyn KernelObject> = channel.clone();

        loop {
            // Wait for a message to arrive
            let signal = channel_obj
                .wait_signal(Signal::READABLE | Signal::PEER_CLOSED)
                .await;
            if signal.contains(Signal::PEER_CLOSED) {
                break;
            }

            // Read all available messages and echo them back
            while let Ok(msg) = channel.read() {
                // Echo the message back — the txid in bytes[0..4] is preserved,
                // which triggers Channel::write's call_reply dispatch.
                let _ = channel.write(msg);
            }
        }

        server
            .state
            .store(WAITER_STATE_WAIT_RETURNED, Ordering::Release);
    })
}

/// An echo server running on a Zircon thread.
pub struct EchoServer {
    pub thread: Arc<Thread>,
    pub state: Arc<EchoServerState>,
    /// The client end of the channel (for the test to use).
    pub client: Arc<Channel>,
}

impl EchoServer {
    /// Create a channel pair and start an echo server on the server end.
    pub fn start(proc: &Arc<Process>) -> Self {
        let (client, server_channel) = Channel::create();
        let server_state = EchoServerState::new(server_channel);
        let thread = Thread::create_with_ext(proc, "echo-server", server_state.clone()).unwrap();
        thread.start(echo_server_thread_fn).unwrap();
        EchoServer {
            thread,
            state: server_state,
            client,
        }
    }

    /// Shut down the echo server by dropping the client channel
    /// (causing PEER_CLOSED) and waiting for the thread to exit.
    pub async fn shutdown(self) {
        // Drop client to send PEER_CLOSED to server
        drop(self.client);
        let deadline = std::time::Instant::now() + core::time::Duration::from_secs(5);
        loop {
            if self.thread.state() == zircon_object::task::ThreadState::Dead {
                break;
            }
            if std::time::Instant::now() >= deadline {
                break;
            }
            async_std::task::yield_now().await;
        }
    }
}

// ── UserPagerHelper ──────────────────────────────────────────────────
//
// Rust equivalent of the C++ UserPager test helper class from
// zircon/system/utest/core/pager/userpager.h.
//
// Provides a convenient API for creating pager-backed VMOs, supplying
// pages with auto-generated content, waiting for page requests, and
// failing pages.

use zircon_object::signal::{
    Pager, Port, PayloadRepr, PortPacketRepr, ZX_PAGER_VMO_READ,
};
use zircon_object::vm::{VmObject, PAGE_SIZE};
use zircon_object::ZxError;

/// Test helper that wraps a Pager + Port pair and provides convenient
/// methods for creating pager-backed VMOs, supplying pages, waiting
/// for page requests, and failing pages.
///
/// Rust equivalent of the C++ `UserPager` class in
/// `zircon/system/utest/core/pager/userpager.h`.
pub struct UserPagerHelper {
    pub pager: Arc<Pager>,
    pub port: Arc<Port>,
    next_key: u64,
}

impl UserPagerHelper {
    /// Create a new UserPagerHelper with a fresh Pager and Port.
    ///
    /// C++ equivalent: `UserPager::Init()`
    pub fn new() -> Self {
        UserPagerHelper {
            pager: Pager::new(),
            port: Port::new(0).unwrap(),
            next_key: 2, // Start at 2, matching C++ kShutdownKey + 1
        }
    }

    /// Create a pager-backed VMO with `num_pages` pages.
    ///
    /// Returns `(vmo, key)` where `key` is the port packet key used
    /// for page request identification.
    ///
    /// C++ equivalent: `UserPager::CreateVmo(num_pages, &vmo)`
    pub fn create_vmo(&mut self, num_pages: usize) -> (Arc<VmObject>, u64) {
        let key = self.next_key;
        self.next_key += 1;
        let size = (num_pages * PAGE_SIZE) as u64;
        let vmo = self
            .pager
            .create_vmo(0, &self.port, key, size)
            .expect("UserPagerHelper::create_vmo failed");
        (vmo, key)
    }

    /// Create a pager-backed VMO with the given options and `num_pages` pages.
    ///
    /// Returns `(vmo, key)` where `key` is the port packet key.
    ///
    /// C++ equivalent: `UserPager::CreateVmoWithOptions(num_pages, options, &vmo)`
    pub fn create_vmo_with_options(
        &mut self,
        num_pages: usize,
        options: u32,
    ) -> (Arc<VmObject>, u64) {
        let key = self.next_key;
        self.next_key += 1;
        let size = (num_pages * PAGE_SIZE) as u64;
        let vmo = self
            .pager
            .create_vmo(options, &self.port, key, size)
            .expect("UserPagerHelper::create_vmo_with_options failed");
        (vmo, key)
    }

    /// Generate buffer contents for a given key and page offset.
    ///
    /// C++ equivalent: `Vmo::GenerateBufferContents()`
    ///
    /// The C++ version fills each `u64` in the page with
    /// `(key << 32) | page_offset`. We replicate that pattern:
    /// each 8-byte word in the page is set to `(key << 32) | page_index`.
    fn generate_page_content(key: u64, page_index: u64) -> alloc::vec::Vec<u8> {
        let pattern = (key << 32) | page_index;
        let words_per_page = PAGE_SIZE / 8;
        let mut buf = alloc::vec::Vec::with_capacity(PAGE_SIZE);
        for _ in 0..words_per_page {
            buf.extend_from_slice(&pattern.to_ne_bytes());
        }
        buf
    }

    /// Supply pages to a VMO range with auto-generated content.
    ///
    /// Each page is filled with `(key << 32) | page_index` in every
    /// 8-byte word, matching the C++ `GenerateBufferContents` pattern.
    ///
    /// C++ equivalent: `UserPager::SupplyPages(vmo, page_offset, page_count)`
    pub fn supply_pages(
        &self,
        vmo: &Arc<VmObject>,
        key: u64,
        page_offset: u64,
        page_count: u64,
    ) {
        let src = VmObject::new_paged(page_count as usize);
        for i in 0..page_count {
            let page_index = page_offset + i;
            let content = Self::generate_page_content(key, page_index);
            src.write(i as usize * PAGE_SIZE, &content)
                .expect("UserPagerHelper: failed to write to source VMO");
        }
        let byte_offset = page_offset * PAGE_SIZE as u64;
        let byte_length = page_count * PAGE_SIZE as u64;
        self.pager
            .supply_pages(vmo, byte_offset, byte_length, &src, 0)
            .expect("UserPagerHelper::supply_pages failed");
    }

    /// Wait for a `ZX_PAGER_VMO_READ` request on the port.
    ///
    /// Returns `(key, offset, length)` from the page request packet.
    ///
    /// C++ equivalent: `UserPager::GetPageReadRequest()`
    pub async fn wait_for_page_read(&self) -> (u64, u64, u64) {
        let packet = self.port.wait().await;
        let repr = PortPacketRepr::from(&packet);
        match repr.data {
            PayloadRepr::PageRequest(pr) => {
                assert_eq!(
                    pr.command, ZX_PAGER_VMO_READ,
                    "expected ZX_PAGER_VMO_READ, got command {}",
                    pr.command
                );
                (repr.key, pr.offset, pr.length)
            }
            _ => panic!(
                "expected PageRequest packet, got {:?}",
                packet.type_
            ),
        }
    }

    /// Supply pages in response to a page read request (convenience).
    ///
    /// Waits for a read request, then supplies the requested pages
    /// with auto-generated content.
    ///
    /// C++ equivalent: combination of `GetPageReadRequest` + `SupplyPages`
    pub async fn handle_page_read(&self, vmo: &Arc<VmObject>, key: u64) {
        let (req_key, offset, length) = self.wait_for_page_read().await;
        assert_eq!(
            req_key, key,
            "page read key mismatch: expected {key}, got {req_key}"
        );
        let page_offset = offset / PAGE_SIZE as u64;
        let page_count = length / PAGE_SIZE as u64;
        self.supply_pages(vmo, key, page_offset, page_count);
    }

    /// Fail pages in the specified range.
    ///
    /// C++ equivalent: `UserPager::FailPages(vmo, page_offset, page_count, error)`
    pub fn fail_pages(
        &self,
        vmo: &Arc<VmObject>,
        page_offset: u64,
        page_count: u64,
        err: ZxError,
    ) {
        let byte_offset = page_offset * PAGE_SIZE as u64;
        let byte_length = page_count * PAGE_SIZE as u64;
        // Map ZxError to the Fuchsia error code expected by op_range.
        let data = match err {
            ZxError::NO_SPACE => (-54i32) as u64,
            ZxError::IO => (-5i32) as u64,
            ZxError::IO_DATA_INTEGRITY => (-29i32) as u64,
            ZxError::BAD_STATE => (-45i32) as u64,
            _ => (-5i32) as u64, // default to ZX_ERR_IO
        };
        // ZX_PAGER_OP_FAIL = 1
        self.pager
            .op_range(1, vmo, byte_offset, byte_length, data)
            .expect("UserPagerHelper::fail_pages failed");
    }

    /// Detach a VMO from the pager.
    ///
    /// C++ equivalent: `UserPager::DetachVmo(vmo)`
    pub fn detach_vmo(&self, vmo: &Arc<VmObject>) {
        self.pager
            .detach_vmo(vmo)
            .expect("UserPagerHelper::detach_vmo failed");
    }

    /// Verify the content of a VMO page matches the expected pattern.
    ///
    /// Returns `true` if the page content matches `generate_page_content(key, page_index)`.
    ///
    /// C++ equivalent: `Vmo::CheckVmo(page_offset, page_count)`
    pub fn check_page(vmo: &Arc<VmObject>, key: u64, page_index: u64) -> bool {
        let expected = Self::generate_page_content(key, page_index);
        let mut buf = alloc::vec![0u8; PAGE_SIZE];
        if vmo.read(page_index as usize * PAGE_SIZE, &mut buf).is_err() {
            return false;
        }
        buf == expected
    }
}

/// A test context providing a Zircon process and current thread.
pub struct TestContext {
    pub proc: Arc<Process>,
    pub current: CurrentThread,
    /// The root job for this test. Stored so it stays alive while the
    /// process exists, and gets killed on drop.
    job: Arc<Job>,
}

impl Drop for TestContext {
    fn drop(&mut self) {
        // Kill the process and job to release all handles, which drops
        // all kernel objects (VMOs, channels, etc.) and frees their
        // physical frames back to the allocator.
        self.proc.kill();
        self.job.kill();
    }
}

impl TestContext {
    /// Create a minimal Zircon process/thread context for testing.
    pub fn new() -> Self {
        ensure_hal_init();
        let job = Job::root();
        let proc = Process::create(&job, "test-proc").unwrap();
        let thread = Thread::create(&proc, "test-thread").unwrap();
        let current = CurrentThread::new_for_test(thread);
        TestContext { proc, current, job }
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
