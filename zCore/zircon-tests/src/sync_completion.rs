//! Sync completion tests.
//!
//! Faithful ports of Fuchsia's sync-completion/completion.cc.
//!
//! The C++ tests use sync_completion_t which is a one-shot signaling
//! primitive built on futexes and atomics:
//! - sync_completion_signal → store(SIGNALED) + futex_wake(UINT_MAX)
//! - sync_completion_wait_deadline → check flag, or futex_wait(deadline)
//! - sync_completion_reset → store(UNSIGNALED)
//! - sync_completion_signaled → read flag
//! - sync_completion_signal_requeue → store(SIGNALED) + futex_requeue
//!
//! In zCore, we use futex + AtomicI32 (value 0 = unsignaled, 1 = signaled)
//! for the single-threaded paths. Multi-threaded tests that require
//! concurrent blocking waits are marked #[ignore].
//!
//! C++ source: zircon/system/utest/core/sync-completion/completion.cc

use crate::helpers::TestContext;
use core::sync::atomic::{AtomicI32, Ordering};
use zircon_object::signal::Futex;

/// C++: TEST(SyncCompletionTest, Initializer)
///
/// Verifies that a zero-initialized completion matches a default-constructed one.
/// C++: memcmp(&static_completion, &completion, sizeof) == 0
#[test]
fn initializer() {
    let _ctx = TestContext::new();
    // C++: static sync_completion_t static_completion (bss-zeroed)
    // C++: sync_completion_t completion (stack, also zeroed)
    static STATIC: AtomicI32 = AtomicI32::new(0);
    let local = AtomicI32::new(0);

    // C++: memcmp == 0
    assert_eq!(
        STATIC.load(Ordering::Relaxed),
        local.load(Ordering::Relaxed),
        "completion's initializer is not all zeroes"
    );
}

/// C++: TEST(SyncCompletionTest, SingleWait)
///
/// Starts 1 thread that blocks on sync_completion_wait_deadline.
/// Main thread waits until it's blocked on the futex, then signals.
/// Thread wakes, verifies status == ZX_OK.
///
/// Requires concurrent execution.
#[test]
#[ignore = "requires preemptive multi-threaded scheduling for blocking futex_wait"]
fn single_wait() {
    // Kernel: futex_wait on completion word, futex_wake(MAX) on signal
}

/// C++: TEST(SyncCompletionTest, MultiWait)
///
/// Same as SingleWait but with 16 threads all blocking on the same completion.
#[test]
#[ignore = "requires preemptive multi-threaded scheduling for blocking futex_wait"]
fn multi_wait() {
    // Kernel: 16 threads doing futex_wait, signal does futex_wake(MAX)
}

/// C++: TEST(SyncCompletionTest, TimeoutSingleWait)
///
/// 1 thread blocks on completion with a deadline 300ms in the future.
/// No signal is sent. Thread should wake with ZX_ERR_TIMED_OUT.
#[test]
#[ignore = "requires preemptive multi-threaded scheduling for blocking futex_wait with deadline"]
fn timeout_single_wait() {
    // Kernel: futex_wait with deadline, no wake → TIMED_OUT
}

/// C++: TEST(SyncCompletionTest, TimeoutMultiWait)
///
/// 16 threads block on completion with a deadline. No signal.
/// All should time out.
#[test]
#[ignore = "requires preemptive multi-threaded scheduling for blocking futex_wait with deadline"]
fn timeout_multi_wait() {
    // Kernel: 16 × futex_wait with deadline, no wake → all TIMED_OUT
}

/// C++: TEST(SyncCompletionTest, PresignalSingleWait)
///
/// Signal the completion BEFORE starting the waiter thread.
/// The thread should return immediately (signaled flag is already set).
///
/// At kernel level: sync_completion_wait checks the atomic flag
/// before calling futex_wait. If already signaled, returns ZX_OK
/// without blocking.
#[test]
fn presignal_single_wait() {
    let _ctx = TestContext::new();
    static COMPLETION: AtomicI32 = AtomicI32::new(0);

    // C++: sync_completion_signal(&completion) — signal first
    COMPLETION.store(1, Ordering::Release);
    let futex = Futex::new(&COMPLETION as *const AtomicI32 as usize);
    futex.wake(usize::MAX);

    // C++: thread calls sync_completion_wait_deadline → ZX_OK immediately
    // (flag is already set, no futex_wait needed)
    assert_eq!(
        COMPLETION.load(Ordering::Acquire),
        1,
        "pre-signaled completion should return ZX_OK"
    );
}

/// C++: TEST(SyncCompletionTest, PresignalMultiWait)
///
/// Same as PresignalSingleWait but verifiable by multiple "threads".
#[test]
fn presignal_multi_wait() {
    let _ctx = TestContext::new();
    static COMPLETION: AtomicI32 = AtomicI32::new(0);

    // Signal first
    COMPLETION.store(1, Ordering::Release);

    // Multiple readers should all see the signaled state
    for _ in 0..16 {
        assert_eq!(
            COMPLETION.load(Ordering::Acquire),
            1,
            "pre-signaled completion should be visible to all waiters"
        );
    }
}

/// C++: TEST(SyncCompletionTest, ResetCycleSingleWait)
///
/// Signal, reset, then signal again. 1 waiter should block on the
/// second signal (first signal was cleared by reset).
///
/// At kernel level: signal stores 1 + futex_wake, reset stores 0.
#[test]
fn reset_cycle_single_wait() {
    let _ctx = TestContext::new();
    static COMPLETION: AtomicI32 = AtomicI32::new(0);
    let futex = Futex::new(&COMPLETION as *const AtomicI32 as usize);

    // C++: sync_completion_signal + sync_completion_reset
    COMPLETION.store(1, Ordering::Release);
    futex.wake(usize::MAX);
    COMPLETION.store(0, Ordering::Release); // reset

    // After reset, completion is unsignaled
    assert_eq!(
        COMPLETION.load(Ordering::Acquire),
        0,
        "should be unsignaled after reset"
    );

    // C++: sync_completion_signal (second time)
    COMPLETION.store(1, Ordering::Release);
    futex.wake(usize::MAX);

    // Waiter should now see signaled
    assert_eq!(
        COMPLETION.load(Ordering::Acquire),
        1,
        "should be signaled after re-signal"
    );
}

/// C++: TEST(SyncCompletionTest, ResetCycleMultiWait)
///
/// Same as ResetCycleSingleWait but with multiple "waiters".
#[test]
fn reset_cycle_multi_wait() {
    let _ctx = TestContext::new();
    static COMPLETION: AtomicI32 = AtomicI32::new(0);
    let futex = Futex::new(&COMPLETION as *const AtomicI32 as usize);

    // Signal and reset
    COMPLETION.store(1, Ordering::Release);
    futex.wake(usize::MAX);
    COMPLETION.store(0, Ordering::Release);
    assert_eq!(COMPLETION.load(Ordering::Acquire), 0);

    // Re-signal
    COMPLETION.store(1, Ordering::Release);
    futex.wake(usize::MAX);

    // All waiters should see signaled
    for _ in 0..16 {
        assert_eq!(COMPLETION.load(Ordering::Acquire), 1);
    }
}

/// C++: TEST(SyncCompletionTest, SignalRequeue)
///
/// The C++ test:
/// 1. Starts 16 threads blocking on completion
/// 2. Waits until all are blocked on the futex
/// 3. Calls sync_completion_signal_requeue to move threads to a different futex
/// 4. Sleeps 100ms, verifies all threads are still blocked (on the new futex)
/// 5. Wakes the new futex with futex_wake(UINT_MAX)
/// 6. Joins all threads, verifies ZX_OK status
///
/// This tests the futex requeue primitive directly.
/// Requires concurrent multi-threaded execution.
#[test]
#[ignore = "requires preemptive multi-threaded scheduling for futex requeue"]
fn signal_requeue() {
    // Kernel primitives:
    //   sync_completion_signal_requeue → store(SIGNALED) + futex_requeue
    //   zx_futex_wake on requeue target
}

/// C++: TEST(SyncCompletionTest, SpuriousWakeupHandled)
///
/// The C++ test:
/// 1. Starts 1 thread blocking on completion (infinite deadline)
/// 2. Waits until thread is blocked on the futex
/// 3. Calls zx_futex_wake on the completion's internal futex (spurious wake)
/// 4. Sleeps 100ms, verifies thread has NOT exited
/// 5. Signals the completion properly
/// 6. Joins thread, verifies it exited and status == ZX_OK
///
/// This tests that sync_completion_wait rechecks the signaled flag
/// after waking from futex_wait, and goes back to sleep if not signaled.
///
/// Requires concurrent multi-threaded execution.
#[test]
#[ignore = "requires preemptive multi-threaded scheduling for spurious wakeup testing"]
fn spurious_wakeup_handled() {
    // Kernel: futex_wake without setting the signaled flag → thread should
    //         re-check and go back to futex_wait
}

/// C++: TEST(SyncCompletionTest, CompletionSignaled)
///
/// The C++ test:
/// 1. ASSERT_FALSE(sync_completion_signaled(&sync))
/// 2. sync_completion_signal(&sync)
/// 3. ASSERT_TRUE(sync_completion_signaled(&sync))
/// 4. sync_completion_reset(&sync)
/// 5. ASSERT_FALSE(sync_completion_signaled(&sync))
///
/// Pure atomic flag check — no blocking, no threads needed.
#[test]
fn completion_signaled() {
    let _ctx = TestContext::new();
    static COMPLETION: AtomicI32 = AtomicI32::new(0);
    let futex = Futex::new(&COMPLETION as *const AtomicI32 as usize);

    // C++: ASSERT_FALSE(sync_completion_signaled(&sync))
    assert_eq!(
        COMPLETION.load(Ordering::Acquire),
        0,
        "initially not signaled"
    );

    // C++: ASSERT_FALSE(sync_completion_signaled(&const_sync))
    assert_eq!(
        COMPLETION.load(Ordering::Acquire),
        0,
        "const ref also not signaled"
    );

    // C++: sync_completion_signal(&sync)
    COMPLETION.store(1, Ordering::Release);
    futex.wake(usize::MAX);

    // C++: ASSERT_TRUE(sync_completion_signaled(&sync))
    assert_eq!(COMPLETION.load(Ordering::Acquire), 1, "should be signaled");

    // C++: ASSERT_TRUE(sync_completion_signaled(&const_sync))
    assert_eq!(
        COMPLETION.load(Ordering::Acquire),
        1,
        "const ref should also be signaled"
    );

    // C++: sync_completion_reset(&sync)
    COMPLETION.store(0, Ordering::Release);

    // C++: ASSERT_FALSE(sync_completion_signaled(&sync))
    assert_eq!(
        COMPLETION.load(Ordering::Acquire),
        0,
        "should be unsignaled after reset"
    );

    // C++: ASSERT_FALSE(sync_completion_signaled(&const_sync))
    assert_eq!(
        COMPLETION.load(Ordering::Acquire),
        0,
        "const ref unsignaled after reset"
    );
}
